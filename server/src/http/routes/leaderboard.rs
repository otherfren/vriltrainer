//! `GET /api/leaderboard`, sorted by the Wilson lower bound, which is also the
//! figure displayed: a board sorted by an invisible statistic produces endless argument (D20).
//!
//! Public, and everything on it is checkable. The name is the last one a human approved or a
//! fixed-length mask (D25, FR-047), the public identifier stands beside it either way so a masked
//! row is still attributable against the log (FR-029), and the band is a share of the eligible
//! population rather than a seat (D23, FR-042).

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
use crate::stats::{measures, ranks};

/// Entries per page when the client does not say. The client pages at twenty; this is the same
/// number so that an unparameterised request and the first page are the same request.
const DEFAULT_LIMIT: u64 = 20;

/// The most a caller may ask for at once. The board is public and uncached, so this is what keeps a
/// single request from serialising a hundred thousand accounts.
const MAX_LIMIT: u64 = 100;

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
    offset: u64,
    limit: u64,
    entries: Vec<Entry>,
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
    /// The supporting figures FR-041 requires beside it. These describe the account's record and
    /// are reported live; `wilson_lower` and `deviation` are inferences and advance per block
    /// (FR-019), which is why the two can disagree about `n` and both be right.
    completed: u64,
    hit_rate: f64,
    deviation: f64,
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
    let entries = read_page(&reader, offset, limit)?;
    let ranks_updated_at = ranks::last_computed(&reader)?.unwrap_or(now);

    Ok(Json(Board {
        eligible_accounts,
        bands_active: ranks::active(eligible_accounts, &cfg.thresholds),
        ranks_updated_at,
        offset,
        limit,
        entries,
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

/// One page of the board, in the order the rank pass numbered places in.
///
/// The place is the row's position in that order, offset and all — never a second sort, because a
/// second sort is a second chance for the board and the ranks to disagree about who is first.
fn read_page(reader: &Connection, offset: u64, limit: u64) -> Result<Vec<Entry>, DbError> {
    let mut stmt = reader.prepare(&format!(
        "SELECT a.public_id, a.public_name, s.completed, s.hits, s.wilson_lower, s.deviation,
                s.rank_slug
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
        Ok(Entry {
            place: 0,
            band: r.get(6)?,
            name: public_display(public_name.as_deref()),
            public_id: r.get(0)?,
            wilson_lower: r.get(4)?,
            completed,
            hit_rate: measures::hit_rate(hits, completed),
            deviation: r.get(5)?,
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

    /// D23: the ladder fills in from the middle outward, and the rarest title stays unminted until
    /// the population can hold one without rounding (SC-013, FR-042).
    #[tokio::test]
    async fn bands_appear_only_once_the_population_can_hold_them() {
        let small = populated(4);
        let body = json(call(&small.state, "/api/leaderboard").await).await;
        assert_eq!(body["eligible_accounts"], 4);
        assert!(
            body["bands_active"].as_array().unwrap().is_empty(),
            "four eligible accounts is not a leaderboard"
        );
        for entry in body["entries"].as_array().unwrap() {
            assert!(entry.get("band").is_none(), "a title at four accounts");
        }

        let bigger = populated(5);
        let body = json(call(&bigger.state, "/api/leaderboard").await).await;
        assert_eq!(body["bands_active"], serde_json::json!(["asset"]));
        let entries = body["entries"].as_array().unwrap();
        assert_eq!(entries[0]["band"], "asset");
        assert_eq!(entries[4]["band"], "pineal", "the ladder is symmetric");
        assert!(entries[2].get("band").is_none(), "the middle is Normie");
    }

    /// The board states the numbers it was computed under (D26, FR-050), and when the ranks last
    /// moved (D23).
    #[tokio::test]
    async fn the_board_reports_the_rules_in_force() {
        let f = populated(5);
        let body = json(call(&f.state, "/api/leaderboard").await).await;
        assert_eq!(body["thresholds"]["eligibility_trials"], 10);
        assert_eq!(body["thresholds"]["eligibility_days"], 3);
        assert_eq!(body["thresholds"]["block_size"], 25);
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

    /// An empty board is still a board: it has to say how many accounts are eligible so the page
    /// can say how far off the first band is (FR-042).
    #[tokio::test]
    async fn an_empty_board_still_states_the_population() {
        let f = Fixture::new();
        let response = call(&f.state, "/api/leaderboard").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;
        assert_eq!(body["eligible_accounts"], 0);
        assert!(body["entries"].as_array().unwrap().is_empty());
        assert!(body["bands_active"].as_array().unwrap().is_empty());
    }
}
