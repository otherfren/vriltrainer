//! `GET /api/leaderboard`, sorted by the Wilson lower bound, which is also the
//! figure displayed: a board sorted by an invisible statistic produces endless argument (D20).
//!
//! Public, and everything on it is checkable. The name is the last one a human approved or a
//! fixed-length mask (D25, FR-047), the public identifier stands beside it either way so a masked
//! row is still attributable against the log (FR-029), and the band is a distance from chance
//! rather than a seat (D31, FR-042).
//!
//! Since D35 the response carries two things beyond the ranked page: `proven` on each entry, the
//! line the page draws its two zones along, and `waiting` — the accounts that have played and have
//! not met the rule yet, with how far short of it they are. Both exist because an empty board and
//! an empty site are otherwise the same page.

use axum::Router;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::account::name::public_display;
use crate::db::{DbError, now_rfc3339};
use crate::http::routes::stats::Published;
use crate::http::{ApiError, AppState};
use crate::stats::{by_chance, measures, ranks};

/// Entries per page when the client does not say. The client pages at twenty; this is the same
/// number so that an unparameterised request and the first page are the same request.
const DEFAULT_LIMIT: u64 = 20;

/// The most a caller may ask for at once. The board is public and uncached, so this is what keeps a
/// single request from serialising a hundred thousand accounts.
const MAX_LIMIT: u64 = 100;

/// How many not-yet-ranked accounts ride along with the first page.
///
/// The board's own rows are the ranked ones; this is the queue behind them, and a queue is only
/// useful while a reader can still see the front of it. Twenty is the page size, so the two lists
/// are the same height.
const WAITING_LIMIT: u64 = 20;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/leaderboard", get(board))
}

#[derive(Deserialize)]
struct Page {
    offset: Option<u64>,
    limit: Option<u64>,
}

#[derive(Serialize)]
struct Board<'a> {
    /// Reported whether or not any band is active, so the board can say how far off the next one is
    /// (FR-042). "Ranks unlock at 50 qualified, currently 47" is a sentence the page can only write
    /// if this number is here.
    eligible_accounts: u64,
    /// The bands that currently exist, widest first — the ladder fills in from the middle outward
    /// as the site grows (D23).
    bands_active: Vec<&'a str>,
    /// When the ranks were last recomputed. Published because a rank that has not moved otherwise
    /// reads as a bug (D23).
    ranks_updated_at: String,
    /// Accounts with a record started and the rule not met yet — the whole population, not the
    /// page. A board that opens early is mostly this number for a while, and hiding it would make
    /// an empty board look like an empty site.
    waiting_accounts: u64,
    offset: u64,
    limit: u64,
    entries: Vec<Entry>,
    /// The front of that queue, first page only. Ranked rows page; this list does not, because it
    /// answers "is anything happening here" and that question is asked once, at the top.
    waiting: Vec<Waiting>,
    thresholds: Published<'a>,
}

#[derive(Serialize)]
struct Entry {
    place: u64,
    /// A band slug, absent for the middle 60 % and for any band the population is still too small
    /// to hold (D23).
    #[serde(skip_serializing_if = "Option::is_none")]
    band: Option<String>,
    /// The most recently **approved** name, or a fixed-length mask (FR-047, D25).
    name: String,
    public_id: String,
    /// The sort key, and the primary figure — a board sorted on something it does not show
    /// produces endless "why is that person above me" (D20, FR-041).
    wilson_lower: f64,
    /// The second sort key, shown for the reason the first one is: below chance `wilson_lower` is
    /// zero at every `n`, so the low tail is ordered entirely on this column and a board that hid
    /// it would be sorted on something it does not show — which is the complaint D20 settled.
    wilson_upper: f64,
    /// The supporting figures FR-041 requires beside it. These describe the account's record and
    /// are reported live; `wilson_lower` and `deviation` are inferences and advance per block
    /// (FR-019), which is why the two can disagree about `n` and both be right.
    completed: u64,
    /// Printed beside the rate rather than left to be inferred from it. "4 of 10" is a sentence a
    /// reader can check; "40 %" over a hidden `n` is the figure that makes a lucky short run look
    /// like a result.
    hits: u64,
    hit_rate: f64,
    /// How many in ten thousand pure guessers reach a record this far from chance.
    ///
    /// This is the figure the board prints where the σ deviation used to be. σ is the right unit
    /// to *compute* the bands in and the wrong one to *read*: it needs a footnote, and a column
    /// nobody can read without one gets read generously on a page about psi (R3).
    ///
    /// Computed live from `hits` and `completed`, which is deliberate and puts it with `hit_rate`
    /// rather than with `deviation`. The point of the column is that a reader can check it against
    /// the two counts printed beside it on the same row; deriving it from the block basis instead
    /// would make the row's own numbers fail to reproduce it.
    by_chance_per_10k: u32,
    deviation: f64,
    /// Whether the assured minimum clears chance — the split the board is drawn in.
    ///
    /// Decided here rather than in the browser for the reason `SigmaBand::tail` is: the line
    /// between "this is more than luck" and "this is consistent with luck" is one definition, and
    /// two implementations of it eventually disagree in public.
    proven: bool,
}

/// An account on its way to the board: a record that exists and a rule not met yet.
#[derive(Serialize)]
struct Waiting {
    /// Masked exactly as a ranked row is (FR-047, D25) — the queue is a public surface too.
    name: String,
    public_id: String,
    completed: u64,
    distinct_days: u32,
    /// Completed trials still missing, zero once only the calendar is outstanding.
    trials_needed: u64,
    /// Distinct days still missing, zero once only the count is outstanding.
    days_needed: u32,
}

async fn board(
    State(state): State<AppState>,
    Query(page): Query<Page>,
) -> Result<Response, ApiError> {
    let cfg = state.config.as_ref();
    let now = now_rfc3339();
    // Compute-on-read, gated on the same fifteen-minute interval the background pass will use. This
    // call is T102's placeholder and goes when it lands; see `ranks::ensure_fresh`.
    ranks::ensure_fresh(&state.db, cfg, &now)?;

    let offset = page.offset.unwrap_or(0);
    let limit = page.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let reader = state.db.reader()?;
    let eligible_accounts = eligible_accounts(&reader)?;
    let waiting_accounts = waiting_accounts(&reader)?;
    let entries = read_page(&reader, offset, limit)?;
    let waiting = if offset == 0 {
        read_waiting(&reader, &cfg.thresholds, WAITING_LIMIT)?
    } else {
        Vec::new()
    };
    let ranks_updated_at = ranks::last_computed(&reader)?.unwrap_or(now);

    Ok(Json(Board {
        eligible_accounts,
        bands_active: ranks::active(&cfg.thresholds),
        ranks_updated_at,
        waiting_accounts,
        offset,
        limit,
        entries,
        waiting,
        thresholds: Published::of(cfg),
    })
    .into_response())
}

fn eligible_accounts(reader: &Connection) -> Result<u64, DbError> {
    let count = reader.query_row(
        "SELECT COUNT(*) FROM account_stats WHERE eligible = 1",
        [],
        |r| r.get(0),
    )?;
    Ok(count)
}

/// Accounts that have played and are not ranked yet.
///
/// `completed > 0` is load-bearing. An account with no trials at all has a lower bound of zero and
/// an upper bound of one, which is the *largest* value of the second sort key — so on any list that
/// admits it, every empty account outranks the worst real record. It is not a queue member either:
/// nothing is in progress there.
fn waiting_accounts(reader: &Connection) -> Result<u64, DbError> {
    let count = reader.query_row(
        "SELECT COUNT(*) FROM account_stats WHERE eligible = 0 AND completed > 0",
        [],
        |r| r.get(0),
    )?;
    Ok(count)
}

/// The front of the queue, longest record first.
///
/// Ordered by evidence collected rather than by rate. Rate is not shown here at all: these records
/// are below the floor by definition, and a "40 %" printed against eight trials is the single most
/// misread number the site could publish.
fn read_waiting(
    reader: &Connection,
    t: &crate::config::Thresholds,
    limit: u64,
) -> Result<Vec<Waiting>, DbError> {
    let mut stmt = reader.prepare(
        "SELECT a.public_id, a.public_name, s.completed, s.distinct_utc_days
           FROM account_stats s JOIN account a ON a.id = s.account_id
          WHERE s.eligible = 0 AND s.completed > 0
          ORDER BY s.completed DESC, s.distinct_utc_days DESC, a.public_id
          LIMIT ?1",
    )?;

    let rows = stmt.query_map(params![limit], |r| {
        let public_name: Option<String> = r.get(1)?;
        let completed: u64 = r.get(2)?;
        let distinct_days: u32 = r.get(3)?;
        Ok(Waiting {
            name: public_display(public_name.as_deref()),
            public_id: r.get(0)?,
            completed,
            distinct_days,
            trials_needed: (t.eligibility_trials as u64).saturating_sub(completed),
            days_needed: t.eligibility_days.saturating_sub(distinct_days),
        })
    })?;

    rows.collect::<rusqlite::Result<Vec<Waiting>>>()
        .map_err(Into::into)
}

/// One page of the board, in the order the rank pass numbered places in.
///
/// The place is the row's position in that order, offset and all — never a second sort, because a
/// second sort is a second chance for the board and the ranks to disagree about who is first.
fn read_page(reader: &Connection, offset: u64, limit: u64) -> Result<Vec<Entry>, DbError> {
    let mut stmt = reader.prepare(&format!(
        "SELECT a.public_id, a.public_name, s.completed, s.hits, s.wilson_lower, s.wilson_upper,
                s.deviation, s.rank_slug
           FROM account_stats s JOIN account a ON a.id = s.account_id
          WHERE s.eligible = 1
          ORDER BY {order}
          LIMIT ?1 OFFSET ?2",
        order = ranks::BOARD_ORDER
    ))?;

    let rows = stmt.query_map(params![limit, offset], |r| {
        let public_name: Option<String> = r.get(1)?;
        let completed: u64 = r.get(2)?;
        let hits: u64 = r.get(3)?;
        let wilson_lower: f64 = r.get(4)?;
        Ok(Entry {
            place: 0,
            band: r.get(7)?,
            name: public_display(public_name.as_deref()),
            public_id: r.get(0)?,
            wilson_lower,
            wilson_upper: r.get(5)?,
            completed,
            hits,
            hit_rate: measures::hit_rate(hits, completed),
            by_chance_per_10k: by_chance::per_10_000(hits, completed),
            deviation: r.get(6)?,
            proven: wilson_lower > measures::CHANCE,
        })
    })?;

    let mut entries = Vec::new();
    for (index, entry) in rows.enumerate() {
        let mut entry = entry?;
        entry.place = offset + index as u64 + 1;
        entries.push(entry);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxumBody;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::account::name::{self, MASK};
    use crate::config::Config;
    use crate::http::routes::stats::test_support::{Fixture, json};
    use crate::http::{AppState, router};

    /// Eligibility is the shipped rule with a lower trial floor, so a test can build a population
    /// without writing tens of thousands of log entries. The day count is untouched — it is the
    /// half of the rule that carries the argument (D21) — and the floor stays at or above the
    /// statistics unlock, or no account would ever reach a block boundary and the whole board
    /// would sort on a column of zeroes.
    fn quick() -> Config {
        let mut cfg = Config::default();
        cfg.thresholds.eligibility_trials = cfg.thresholds.stats_unlock_at;
        cfg
    }

    async fn call(state: &AppState, uri: &str) -> axum::response::Response {
        router(state.clone())
            .oneshot(Request::builder().uri(uri).body(AxumBody::empty()).unwrap())
            .await
            .unwrap()
    }

    /// A board of `n` accounts, each eligible, the first of them the strongest.
    fn populated(n: u32) -> Fixture {
        let mut f = Fixture::with_config(quick());
        for i in 0..n {
            let player = f.player();
            // Twelve trials over three days, past both the unlock and the eligibility floor. The
            // first accounts hit more often than the last, so the board has an order to find.
            f.play_across_days(&player, 4, (n - i).min(8), 3);
        }
        f
    }

    /// D20 and FR-041: the board is ordered by the figure it prints as the primary one.
    #[tokio::test]
    async fn the_board_is_sorted_by_the_bound_it_displays() {
        let f = populated(6);
        let body = json(call(&f.state, "/api/leaderboard").await).await;

        let entries = body["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 6);
        let bounds: Vec<f64> = entries
            .iter()
            .map(|e| e["wilson_lower"].as_f64().unwrap())
            .collect();
        assert!(
            bounds.windows(2).all(|w| w[0] >= w[1]),
            "not sorted by the sort key: {bounds:?}"
        );
        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(entry["place"], i as u64 + 1);
            assert!(entry["completed"].is_number());
            assert!(entry["hit_rate"].is_number());
            assert!(entry["deviation"].is_number());
        }
    }

    /// SC-009 and FR-040: a short lucky run cannot occupy a ranked position, however good it looks.
    #[tokio::test]
    async fn a_perfect_short_run_never_reaches_the_board() {
        let mut f = Fixture::with_config(quick());
        let flash = f.player();
        f.play(&flash, 12, 12);
        let steady = f.player();
        f.play_across_days(&steady, 4, 2, 3);

        let body = json(call(&f.state, "/api/leaderboard").await).await;
        assert_eq!(body["eligible_accounts"], 1);
        let entries = body["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["public_id"], steady.public_id);
    }

    /// FR-047 and D25: an unapproved name is masked, and the public identifier stands beside it so
    /// the row is still attributable against the log (FR-029).
    #[tokio::test]
    async fn an_unapproved_name_is_masked_and_still_attributable() {
        let mut f = Fixture::with_config(quick());
        let reviewed = f.player();
        f.play_across_days(&reviewed, 4, 8, 3);
        let waiting = f.player();
        f.play_across_days(&waiting, 4, 1, 3);
        name::approve(&f.state.db, &reviewed.id, &reviewed.name).unwrap();

        let body = json(call(&f.state, "/api/leaderboard").await).await;
        let entries = body["entries"].as_array().unwrap();
        assert_eq!(entries[0]["name"], reviewed.name);
        assert_eq!(entries[1]["name"], MASK);
        assert_eq!(entries[1]["public_id"], waiting.public_id);
        assert_ne!(
            entries[1]["name"], waiting.name,
            "a name nobody approved reached a public surface"
        );
    }

    /// Every entry holds the band its own deviation earns, and nothing else decides it.
    ///
    /// The one assertion worth making about ranks on this surface since D31. It is deliberately
    /// written against `ranks::band_for` rather than against literal slugs: the point is not which
    /// title a particular fixture happens to produce, it is that the board and the ladder cannot
    /// disagree about the same number.
    fn bands_match_deviations(entries: &[serde_json::Value]) {
        let t = Config::default().thresholds;
        for entry in entries {
            let z = entry["deviation"]
                .as_f64()
                .expect("an entry states its sigma");
            let earned = crate::stats::ranks::band_for(z, &t).map(|a| a.slug().to_owned());
            let held = entry["band"].as_str().map(str::to_owned);
            assert_eq!(
                held, earned,
                "the band on the board is not the one {z} σ earns"
            );
        }
    }

    /// D31, superseding D23: a rung is a distance from chance, so the ladder is the same ladder at
    /// four accounts as at four thousand and a small board can already hand out titles (FR-042).
    ///
    /// This used to assert the opposite — that four eligible accounts is not a leaderboard and gets
    /// no titles at all. That was the share model, under which your rank was a statement about who
    /// else had signed up.
    #[tokio::test]
    async fn the_ladder_does_not_depend_on_the_population() {
        let full = serde_json::json!(["asset", "grey", "reptilian", "loosh", "annunaki"]);

        let small = populated(4);
        let body = json(call(&small.state, "/api/leaderboard").await).await;
        assert_eq!(body["eligible_accounts"], 4);
        assert_eq!(body["bands_active"], full, "the ladder is not a share");
        bands_match_deviations(body["entries"].as_array().unwrap());

        let bigger = populated(5);
        let body = json(call(&bigger.state, "/api/leaderboard").await).await;
        assert_eq!(body["bands_active"], full);
        bands_match_deviations(body["entries"].as_array().unwrap());
    }

    /// The split the board is drawn in is the one the sort key already makes, and it is decided on
    /// the server so the page and the figure cannot disagree about who cleared chance.
    #[tokio::test]
    async fn above_chance_is_marked_on_the_entry_not_left_to_the_page() {
        let mut f = Fixture::with_config(quick());
        // Ten of twelve. Even the assured minimum of a record this short is far above 12.5 %.
        let strong = f.player();
        f.play_across_days(&strong, 4, 10, 3);
        // One of twelve, which is chance and no evidence of anything.
        let ordinary = f.player();
        f.play_across_days(&ordinary, 4, 1, 3);

        let body = json(call(&f.state, "/api/leaderboard").await).await;
        let entries = body["entries"].as_array().unwrap();
        for entry in entries {
            let bound = entry["wilson_lower"].as_f64().unwrap();
            assert_eq!(
                entry["proven"].as_bool().unwrap(),
                bound > 0.125,
                "the mark and the bound disagree at {bound}"
            );
        }
        assert_eq!(entries[0]["public_id"], strong.public_id);
        assert_eq!(entries[0]["proven"], true);
        assert_eq!(entries[1]["proven"], false);
    }

    /// The hits are printed beside the rate, so "4 of 10" can be read instead of inferred.
    #[tokio::test]
    async fn an_entry_states_its_hits_as_well_as_its_rate() {
        let mut f = Fixture::with_config(quick());
        let player = f.player();
        f.play_across_days(&player, 4, 3, 3);

        let body = json(call(&f.state, "/api/leaderboard").await).await;
        let entry = &body["entries"][0];
        let hits = entry["hits"].as_u64().unwrap();
        let completed = entry["completed"].as_u64().unwrap();
        assert_eq!(hits, 3);
        assert_eq!(completed, 12);
        let rate = entry["hit_rate"].as_f64().unwrap();
        assert!(
            (rate - hits as f64 / completed as f64).abs() < 1e-9,
            "the printed hits do not make the printed rate"
        );
    }

    /// The by-chance figure is reproducible from the two counts on its own row.
    ///
    /// That is the whole reason it is computed live rather than off the block basis: it replaced a
    /// σ column nobody could check by eye, and a replacement that also cannot be checked by eye
    /// would not have been worth making.
    #[tokio::test]
    async fn the_by_chance_figure_follows_from_the_counts_printed_beside_it() {
        let mut f = Fixture::with_config(quick());
        let player = f.player();
        f.play_across_days(&player, 4, 3, 3);

        let body = json(call(&f.state, "/api/leaderboard").await).await;
        let entry = &body["entries"][0];
        let hits = entry["hits"].as_u64().unwrap();
        let completed = entry["completed"].as_u64().unwrap();
        assert_eq!(
            entry["by_chance_per_10k"].as_u64().unwrap() as u32,
            crate::stats::by_chance::per_10_000(hits, completed),
            "the board's figure is not the one its own counts produce"
        );
    }

    /// The queue behind the board: played, not ranked, and told how far off it is.
    ///
    /// This is what an early board is mostly made of, and the reason it exists is that "nobody
    /// qualifies yet" and "nobody is playing" look identical otherwise.
    #[tokio::test]
    async fn accounts_short_of_the_rule_ride_along_with_their_distance_to_it() {
        let mut f = Fixture::with_config(quick());
        let ranked = f.player();
        f.play_across_days(&ranked, 4, 3, 3);
        // Eight trials in one sitting: short on both halves of the rule.
        let busy = f.player();
        f.play(&busy, 8, 1);
        // Two trials in one sitting: shorter on the count, same day problem.
        let curious = f.player();
        f.play(&curious, 2, 0);
        // An account that never played is not in the queue — there is nothing in progress.
        let _idle = f.player();

        let body = json(call(&f.state, "/api/leaderboard").await).await;
        assert_eq!(body["eligible_accounts"], 1);
        assert_eq!(body["waiting_accounts"], 2);

        let waiting = body["waiting"].as_array().unwrap();
        assert_eq!(waiting.len(), 2, "the idle account joined the queue");
        assert_eq!(waiting[0]["public_id"], busy.public_id);
        assert_eq!(waiting[0]["completed"], 8);
        assert_eq!(waiting[0]["distinct_days"], 1);
        // The floor is `stats_unlock_at` under `quick()`, so eight trials is two short of it, and
        // one day is one short of the calendar.
        assert_eq!(waiting[0]["trials_needed"], 2);
        assert_eq!(waiting[0]["days_needed"], 1);
        assert_eq!(waiting[1]["public_id"], curious.public_id);
        assert_eq!(waiting[1]["trials_needed"], 8);

        for entry in waiting {
            assert!(
                entry["hit_rate"].is_null(),
                "a rate below the floor reached a public surface"
            );
        }
    }

    /// The queue rides with the first page only: it answers a question asked at the top.
    #[tokio::test]
    async fn the_queue_does_not_repeat_on_every_page() {
        let mut f = populated(3);
        let waiting = f.player();
        f.play(&waiting, 8, 1);

        let first = json(call(&f.state, "/api/leaderboard?limit=2").await).await;
        assert_eq!(first["waiting"].as_array().unwrap().len(), 1);
        assert_eq!(first["waiting_accounts"], 1);

        let second = json(call(&f.state, "/api/leaderboard?offset=2&limit=2").await).await;
        assert!(second["waiting"].as_array().unwrap().is_empty());
        assert_eq!(
            second["waiting_accounts"], 1,
            "the count is the population and does not page"
        );
    }

    /// The board states the numbers it was computed under (D26, FR-050), and when the ranks last
    /// moved (D23).
    #[tokio::test]
    async fn the_board_reports_the_rules_in_force() {
        let f = populated(5);
        let body = json(call(&f.state, "/api/leaderboard").await).await;
        assert_eq!(body["thresholds"]["eligibility_trials"], 10);
        assert_eq!(body["thresholds"]["eligibility_days"], 2);
        assert_eq!(body["thresholds"]["block_size"], 10);
        assert_eq!(body["thresholds"]["bands"][0]["high"], "annunaki");
        assert_eq!(body["thresholds"]["bands"][0]["low"], "kartoffel");
        assert!(body["ranks_updated_at"].as_str().unwrap().ends_with('Z'));
    }

    /// Paging keeps the places running, which is the only reason the number is on the entry rather
    /// than counted by the client.
    #[tokio::test]
    async fn paging_continues_the_places() {
        let f = populated(6);
        let first = json(call(&f.state, "/api/leaderboard?limit=2").await).await;
        let second = json(call(&f.state, "/api/leaderboard?offset=2&limit=2").await).await;

        assert_eq!(first["entries"].as_array().unwrap().len(), 2);
        assert_eq!(first["entries"][0]["place"], 1);
        assert_eq!(second["entries"][0]["place"], 3);
        assert_eq!(second["entries"][1]["place"], 4);
        assert_eq!(second["limit"], 2);
        assert_eq!(second["offset"], 2);
    }

    /// An empty board is still a board: it states the population, and it states the whole ladder
    /// so a first visitor can see what the rungs are called before anybody holds one (FR-042).
    #[tokio::test]
    async fn an_empty_board_still_states_the_population_and_the_ladder() {
        let f = Fixture::new();
        let response = call(&f.state, "/api/leaderboard").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;
        assert_eq!(body["eligible_accounts"], 0);
        assert!(body["entries"].as_array().unwrap().is_empty());
        assert_eq!(
            body["bands_active"],
            serde_json::json!(["asset", "grey", "reptilian", "loosh", "annunaki"])
        );
    }

    /// The low tail is ordered on the *upper* bound, and this is the board that proved it has to be.
    ///
    /// Three accounts with no hits at all, over records of very different lengths. Their
    /// `wilson_lower` is not merely clamped to zero, it is zero exactly at every `n`, so all three
    /// tie on the primary key and whatever follows decides the bottom of the ladder. Ordered by
    /// `completed` — which is what it used to be — the tail came out backwards: the account with
    /// the most evidence of an anti-talent placed *above* the one with the least, and the weakest
    /// result took the low band. Nothing about that looks wrong until somebody reads the sigmas.
    #[tokio::test]
    async fn more_evidence_of_an_anti_talent_places_you_lower_not_higher() {
        let mut f = Fixture::with_config(quick());
        // Four above chance, so a population exists for the bands to be shares of.
        for hits in [8, 7, 6, 5] {
            let p = f.player();
            f.play_across_days(&p, 4, hits, 3);
        }
        // Three at nothing, over 12, 36 and 63 trials. The reported figures stand at the block
        // boundaries below those (10, 35, 60), which is what gives the three distinct bounds.
        let short = f.player();
        f.play_across_days(&short, 4, 0, 3);
        let mid = f.player();
        f.play_across_days(&mid, 12, 0, 3);
        let long = f.player();
        f.play_across_days(&long, 21, 0, 3);

        let body = json(call(&f.state, "/api/leaderboard?limit=100").await).await;
        let entries = body["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 7);

        let tail: Vec<&str> = entries[4..]
            .iter()
            .map(|e| e["public_id"].as_str().unwrap())
            .collect();
        assert_eq!(
            tail,
            [&short.public_id, &mid.public_id, &long.public_id],
            "the low tail is not ordered by the bound that carries information down there"
        );

        for e in &entries[4..] {
            assert_eq!(
                e["wilson_lower"], 0.0,
                "the premise of this test has changed"
            );
        }
        let ceilings: Vec<f64> = entries
            .iter()
            .map(|e| e["wilson_upper"].as_f64().unwrap())
            .collect();
        assert!(
            ceilings.windows(2).all(|w| w[0] >= w[1]),
            "the second sort key is not monotone across the board: {ceilings:?}"
        );

        // The bottom place is the longest record without a hit, and it is a low band because that
        // is what its sigma is — not because it came last. Every other entry is checked the same
        // way, against its own deviation rather than against its place.
        assert_eq!(entries[6]["public_id"], long.public_id);
        assert!(
            entries[6]["band"].is_string(),
            "the low band went missing from the bottom place"
        );
        bands_match_deviations(entries);
    }
}
