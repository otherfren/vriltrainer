//! `GET /api/stats/me` and `GET /api/stats/aggregate`.
//!
//! Two endpoints with opposite jobs. The personal one is the flattering half and is therefore the
//! one with rules on it: gated on completed trials and never on success (D8, SC-006), advancing per
//! block (FR-019), and always carrying the abandoned count so selective abandonment is visible
//! rather than hidden (FR-021).
//!
//! The aggregate is the scientifically load-bearing figure and runs over **every** trial by
//! **every** account, including those of users who never reached the statistics threshold. That is
//! D8's rule in one line — gate the display, never the data — and conditioning this population on
//! anything is what makes a psi site overstate the number it exists to report honestly.

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use rusqlite::params;
use serde::Serialize;

use crate::config::{Config, Thresholds};
use crate::db::{Db, DbError, now_rfc3339};
use crate::http::routes::account::Holder;
use crate::http::{ApiError, AppState};
use crate::stats::spread::{self, Band, TAIL_SIGMA};
use crate::stats::{accumulate, blocks, by_chance, eligibility, measures, ranks};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/stats/me", get(me))
        .route("/api/stats/aggregate", get(aggregate))
}

/// The thresholds in force, reported rather than assumed (D26, FR-050).
///
/// Sent with every response that depends on one, so nobody loses a rank to a number they could not
/// see, and so the client never has to compile a copy of a figure the operator is expected to move.
#[derive(Serialize)]
pub(crate) struct Published<'a> {
    #[serde(flatten)]
    thresholds: &'a Thresholds,
    /// Not part of [`Thresholds`], but the same kind of number: it is what the reported figures
    /// advance in steps of, so a reader who wonders why their z-score has not moved can see why.
    block_size: u32,
}

impl<'a> Published<'a> {
    pub(crate) fn of(cfg: &'a Config) -> Self {
        Published {
            thresholds: &cfg.thresholds,
            block_size: cfg.block_size,
        }
    }
}

/// Before the statistics view unlocks.
#[derive(Serialize)]
struct Locked<'a> {
    completed: u64,
    /// Present here too. FR-021 says "always", and an account that abandons its first nine trials
    /// is exactly the case the count exists to make visible.
    abandoned: u64,
    unlocks_at: u32,
    thresholds: Published<'a>,
}

/// After it unlocks.
#[derive(Serialize)]
struct Mine<'a> {
    completed: u64,
    hits: u64,
    abandoned: u64,
    hit_rate: f64,
    /// The `n` the three inferential figures below stand over, and the hits inside it (FR-019).
    ///
    /// Reported rather than left implicit: `deviation` over 100 trials printed beside a completed
    /// count of 117 invites a reader to divide one by the other, and the answer would be wrong.
    reported_trials: u64,
    reported_hits: u64,
    deviation: f64,
    /// How many of 10,000 reach this by luck alone (R3). Without it the deviation is not
    /// interpretable, and an uninterpretable number on a page about psi is read generously.
    by_chance_per_10k: u32,
    wilson_lower: f64,
    /// The other end of the interval. Published alongside its counterpart because the counterpart
    /// is uninformative below chance — an account with no hits in three hundred trials has a
    /// guaranteed minimum rate of 0.0 %, which is also what an account with no hits in ten has.
    /// The ceiling is what separates them, and it is the figure the low tail is ranked on.
    wilson_upper: f64,
    distinct_days: u32,
    eligible: bool,
    /// A band **slug**, not a position — a rung is a distance from chance, so there is no seat
    /// number to report (D31, FR-042). Absent for the middle band, which is Normie: under chance
    /// about a quarter of everyone, and the honest answer for them.
    #[serde(skip_serializing_if = "Option::is_none")]
    rank: Option<String>,
    unlocks_at: u32,
    thresholds: Published<'a>,
}

/// One account's own figures.
async fn me(State(state): State<AppState>, Holder(account): Holder) -> Result<Response, ApiError> {
    let cfg = state.config.as_ref();
    let now = now_rfc3339();

    // The account's own last trial has to be in this answer, whatever else is stale.
    accumulate::ensure_current(&state.db, cfg, &account, &now)?;
    // For `rank` only. Interval-gated, so this is a read of one column on all but one request in
    // fifteen minutes — see `ranks::ensure_fresh`, which goes when T102 lands.
    ranks::ensure_fresh(&state.db, cfg, &now)?;

    let reader = state.db.reader()?;
    let stats = accumulate::load(&reader, &account)?;
    // Recounted live rather than read from the row: a trial becomes abandoned when its lifetime
    // runs out, which is a fact about the clock and not about anything that was written.
    let abandoned =
        accumulate::recount_abandoned(&reader, &account, &accumulate::still_open_from(&now, cfg))?;

    let unlocks_at = cfg.thresholds.stats_unlock_at;
    // Completed trials alone. An "at least one hit" gate would condition the displayed population
    // on success — at 10 trials a user with no ability scores zero 26.3 % of the time and vanishes,
    // and the survivors would report 17.0 % against a true 12.5 % (D8, FR-017, SC-006).
    if stats.completed < unlocks_at as u64 {
        return Ok(Json(Locked {
            completed: stats.completed,
            abandoned,
            unlocks_at,
            thresholds: Published::of(cfg),
        })
        .into_response());
    }

    let reported_trials =
        blocks::reported(stats.completed, cfg.block_size as u64, unlocks_at as u64);
    let reported_hits = accumulate::hits_within(&reader, &account, reported_trials)?;

    Ok(Json(Mine {
        completed: stats.completed,
        hits: stats.hits,
        abandoned,
        hit_rate: measures::hit_rate(stats.hits, stats.completed),
        reported_trials,
        reported_hits,
        // Read from the row, where they were written at the last block boundary over exactly this
        // pair. Recomputing them here would move the figure mid-block.
        deviation: stats.deviation,
        by_chance_per_10k: by_chance::per_10_000(reported_hits, reported_trials),
        wilson_lower: stats.wilson_lower,
        wilson_upper: stats.wilson_upper,
        distinct_days: stats.distinct_utc_days,
        // Against the thresholds in force rather than the flag the last rank pass wrote, so a floor
        // an operator moved this morning takes effect now (D26).
        eligible: eligibility::is_eligible(
            stats.completed,
            stats.distinct_utc_days,
            &cfg.thresholds,
        ),
        rank: stats.rank,
        unlocks_at,
        thresholds: Published::of(cfg),
    })
    .into_response())
}

/// The headline finding (FR-020, FR-045).
#[derive(Serialize)]
struct Aggregate<'a> {
    trials: u64,
    hits: u64,
    hit_rate: f64,
    /// `1/8`, printed beside the observed rate so the comparison needs no arithmetic. D18 expects
    /// these two to agree, and the site says so plainly when they do (FR-045).
    expected_rate: f64,
    deviation: f64,
    accounts: u64,
    /// FR-027: abandonment is published, not swept. Anyone holding the export can recompute this.
    abandoned: u64,
    /// Committed, unanswered, and still inside its lifetime — so neither a completed trial nor an
    /// abandoned one. Published beside `abandoned` because without it the two figures above do not
    /// add up to the number of trials that were started, and a reader who notices the gap has no way
    /// to tell whether the difference is an unpublished third state or a bug. On a young log this is
    /// where *every* unanswered trial sits, which is why `abandoned` can honestly read zero while
    /// most starts are unanswered.
    open: u64,
    /// The two tails side by side (FR-043, SC-014). Under the null they arrive in roughly equal
    /// numbers; if the site keeps producing about as many Kartoffeln as Annunaki, that ratio *is*
    /// the significance test, readable without statistics.
    tail_high: u64,
    tail_low: u64,
    /// Every qualified account, in sigma bands, most negative first — the tails plus the middle they
    /// are tails of. Published because two counts of zero are the honest answer under the null and
    /// look like a broken page, while a populated middle shows the same finding as a measurement
    /// somebody took. Always the full set of bands, empty ones included.
    distribution: Vec<Band>,
    /// How many accounts the distribution is over. A flat chart with two accounts in it and a flat
    /// chart with two thousand are different findings, and this is the difference.
    qualified: u64,
    /// What "markedly" means here, in standard deviations, and over what minimum record — stated,
    /// because the counts mean nothing without it.
    tail_sigma: f64,
    tail_min_trials: u32,
    /// How long a started trial stays answerable (D16), which is the line `open` and `abandoned` are
    /// the two sides of. Published for the same reason as `tail_sigma`: the two counts are not
    /// interpretable without the cutoff that separates them, and the reader should not have to take
    /// the operator's word for what it is.
    lifetime_hours: i64,
    thresholds: Published<'a>,
}

/// The aggregate over every trial in the log. Public: this is the figure a reader came to check.
async fn aggregate(State(state): State<AppState>) -> Result<Response, ApiError> {
    let cfg = state.config.as_ref();
    let now = now_rfc3339();

    // The tail counts read `account_stats`, so a row the log has moved past would understate them.
    ranks::ensure_fresh(&state.db, cfg, &now)?;

    let totals = read_totals(&state.db, cfg, &now)?;
    let Totals {
        trials,
        hits,
        accounts,
        abandoned,
        open,
        spread,
    } = totals;

    Ok(Json(Aggregate {
        trials,
        hits,
        hit_rate: measures::hit_rate(hits, trials),
        expected_rate: measures::CHANCE,
        // Over the whole log, with no block rule applied. Optional stopping by one user cannot bias
        // a sum over every user, and truncating here would throw away real trials to defend against
        // a bias this figure does not have (D8's fourth measure).
        deviation: measures::deviation(hits, trials),
        accounts,
        abandoned,
        open,
        // Read off the same binning as the chart, so the sentence and the picture cannot disagree.
        tail_high: spread.tail_high,
        tail_low: spread.tail_low,
        qualified: spread.qualified,
        distribution: spread.bands,
        tail_sigma: TAIL_SIGMA,
        tail_min_trials: cfg.thresholds.stats_unlock_at,
        lifetime_hours: cfg.trial_lifetime_hours,
        thresholds: Published::of(cfg),
    })
    .into_response())
}

/// The counts behind the aggregate, read together.
struct Totals {
    trials: u64,
    hits: u64,
    accounts: u64,
    abandoned: u64,
    open: u64,
    spread: spread::Spread,
}

fn read_totals(db: &Db, cfg: &Config, now: &str) -> Result<Totals, DbError> {
    let reader = db.reader()?;

    let (trials, hits, accounts) = reader.query_row(
        "SELECT COUNT(*), COALESCE(SUM(hit), 0), COUNT(DISTINCT account_id)
           FROM log_entry WHERE kind = 'resolve'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    // A commit with no resolve, split on the one cutoff: before it the trial can no longer be
    // answered and is abandoned, at or after it the trial is merely open. Both sides are counted
    // against the same string, so every unanswered commit lands on exactly one of them and
    // `trials + abandoned + open` is the number of trials that were ever started.
    let still_open_from = accumulate::still_open_from(now, cfg);
    let abandoned = reader.query_row(
        "SELECT COUNT(*) FROM log_entry c
          WHERE c.kind = 'commit' AND c.at < ?1
            AND NOT EXISTS (SELECT 1 FROM log_entry r
                             WHERE r.trial_id = c.trial_id AND r.kind = 'resolve')",
        params![&still_open_from],
        |r| r.get(0),
    )?;
    let open = reader.query_row(
        "SELECT COUNT(*) FROM log_entry c
          WHERE c.kind = 'commit' AND c.at >= ?1
            AND NOT EXISTS (SELECT 1 FROM log_entry r
                             WHERE r.trial_id = c.trial_id AND r.kind = 'resolve')",
        params![&still_open_from],
        |r| r.get(0),
    )?;

    // Every account with a long enough record for a deviation to mean anything, which is a gate on
    // trial count and never on outcome — the same rule as the statistics view (D8). Binned in Rust
    // rather than by a SQL expression per band: the bands have to be cut mirror-symmetrically for
    // the two tails to be comparable, and one function that does it is one place to be wrong.
    let deviations = reader
        .prepare("SELECT deviation FROM account_stats WHERE completed >= ?1")?
        .query_map(params![cfg.thresholds.stats_unlock_at], |r| r.get(0))?
        .collect::<Result<Vec<f64>, _>>()?;

    Ok(Totals {
        trials,
        hits,
        accounts,
        abandoned,
        open,
        spread: spread::of(&deviations, &cfg.thresholds),
    })
}

/// A database with accounts and played trials in it, shared with the leaderboard's tests.
///
/// It writes `COMMIT`/`RESOLVE` pairs straight to the log and folds each one in with
/// [`accumulate::on_resolve`], which is what the answer path does — driving the real loop would
/// need a pool that can fill a trial and a wall clock that can be moved past the minimum viewing
/// time, neither of which says anything about statistics.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Arc;

    use crate::account;
    use crate::config::Config;
    use crate::http::AppState;
    use crate::log::chain::Body;
    use crate::stats::accumulate;

    pub(crate) struct Player {
        pub(crate) id: String,
        pub(crate) token: String,
        pub(crate) public_id: String,
        pub(crate) name: String,
    }

    pub(crate) struct Fixture {
        pub(crate) state: AppState,
        /// Distinct trial identifiers, and the minute of the day each entry is stamped with. Kept
        /// monotonic because sequence order is time order everywhere else in this codebase.
        clock: u32,
        players: u32,
    }

    impl Fixture {
        pub(crate) fn new() -> Self {
            Fixture::with_config(Config::default())
        }

        pub(crate) fn with_config(config: Config) -> Self {
            let state = AppState {
                config: Arc::new(config),
                ..crate::http::test_support::state()
            };
            Fixture {
                state,
                clock: 0,
                players: 0,
            }
        }

        pub(crate) fn player(&mut self) -> Player {
            self.players += 1;
            let name = format!("viewer{}", self.players);
            let created = account::create(&self.state.db, &name, "2026-07-01T00:00:00Z")
                .expect("the fixture name passes the filter");
            Player {
                id: created.id,
                token: created.access_token,
                public_id: created.public_id,
                name: created.name,
            }
        }

        /// `trials` completed trials on one day, `hits` of them correct.
        pub(crate) fn play(&mut self, player: &Player, trials: u32, hits: u32) {
            self.play_on(player, trials, hits, 1);
        }

        /// The same, spread over `days` distinct UTC days — which is what leaderboard eligibility
        /// counts (FR-040, R4).
        pub(crate) fn play_across_days(
            &mut self,
            player: &Player,
            per_day: u32,
            hits: u32,
            days: u32,
        ) {
            let mut left = hits;
            for day in 1..=days {
                let today = left.min(per_day);
                self.play_on(player, per_day, today, day);
                left -= today;
            }
        }

        fn play_on(&mut self, player: &Player, trials: u32, hits: u32, day: u32) {
            for i in 0..trials {
                let at = self.stamp(day);
                let trial = self.commit(player, &at);
                let hit = i < hits;
                let config = Arc::clone(&self.state.config);
                let account = player.id.clone();
                self.state
                    .db
                    .append_with(
                        &at,
                        Body::Resolve {
                            trial,
                            chosen: "img_1".into(),
                            target: if hit { "img_1".into() } else { "img_2".into() },
                            hit,
                            s_server: "aa".into(),
                            s_client: "bb".into(),
                            nonce: "cc".into(),
                        },
                        |tx, entry| accumulate::on_resolve(tx, &config, &account, hit, &entry.at),
                    )
                    .expect("the fixture writes a well-formed resolve");
            }
        }

        /// Trials started and never answered, stamped far enough back that their lifetime has
        /// certainly run out.
        pub(crate) fn abandon(&mut self, player: &Player, trials: u32) {
            for _ in 0..trials {
                self.commit(player, "2026-06-15T09:00:00Z");
            }
        }

        /// Trials started and not answered *yet*, stamped now so their lifetime is certainly still
        /// running. These are open, not abandoned, and the difference is the whole point of
        /// publishing both counts.
        pub(crate) fn leave_open(&mut self, player: &Player, trials: u32) {
            for _ in 0..trials {
                let at = crate::db::now_rfc3339();
                self.commit(player, &at);
            }
        }

        fn commit(&mut self, player: &Player, at: &str) -> String {
            self.clock += 1;
            let trial = format!("trial{:06}", self.clock);
            self.state
                .db
                .append(
                    at,
                    Body::Commit {
                        trial: trial.clone(),
                        account: player.id.clone(),
                        coordinate: "4821-9037".into(),
                        commitment: "sha256:aa".into(),
                        pool_version: 1,
                        pool_manifest_hash: Some("sha256:pool".into()),
                    },
                )
                .expect("the fixture writes a well-formed commit");
            trial
        }

        /// A timestamp on `day` of July 2026, always in the past relative to the process clock so
        /// that an unanswered trial is unambiguously abandoned rather than merely open.
        fn stamp(&self, day: u32) -> String {
            let n = self.clock;
            format!(
                "2026-07-{:02}T{:02}:{:02}:{:02}Z",
                day,
                (n / 3600) % 24,
                (n / 60) % 60,
                n % 60
            )
        }
    }

    pub(crate) async fn json(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxumBody;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::test_support::{Fixture, json};
    use crate::http::{AppState, router};

    async fn call(state: &AppState, uri: &str, token: Option<&str>) -> axum::response::Response {
        let mut request = Request::builder().method("GET").uri(uri);
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        router(state.clone())
            .oneshot(request.body(AxumBody::empty()).unwrap())
            .await
            .unwrap()
    }

    /// FR-017 and FR-021: below the threshold the page says how far off it is, and still says how
    /// many trials were walked away from.
    #[tokio::test]
    async fn the_locked_view_reports_the_threshold_and_the_abandoned_count() {
        let mut f = Fixture::new();
        let player = f.player();
        f.play(&player, 4, 0);
        f.abandon(&player, 2);

        let body = json(call(&f.state, "/api/stats/me", Some(&player.token)).await).await;
        assert_eq!(body["completed"], 4);
        assert_eq!(body["abandoned"], 2);
        assert_eq!(body["unlocks_at"], 10);
        assert!(
            body["hit_rate"].is_null(),
            "no figures before the threshold"
        );
        assert_eq!(body["thresholds"]["eligibility_days"], 2);
    }

    /// SC-006, the one the whole statistics design turns on. Two accounts with the same number of
    /// completed trials see the same shape of page, whether one of them has ever hit or not.
    #[tokio::test]
    async fn the_gate_is_trial_count_alone_and_never_success() {
        let mut f = Fixture::new();
        let lucky = f.player();
        let hopeless = f.player();
        f.play(&lucky, 10, 10);
        f.play(&hopeless, 10, 0);

        let a = json(call(&f.state, "/api/stats/me", Some(&lucky.token)).await).await;
        let b = json(call(&f.state, "/api/stats/me", Some(&hopeless.token)).await).await;

        let mut keys_a: Vec<&str> = a.as_object().unwrap().keys().map(String::as_str).collect();
        let mut keys_b: Vec<&str> = b.as_object().unwrap().keys().map(String::as_str).collect();
        keys_a.sort_unstable();
        keys_b.sort_unstable();
        assert_eq!(keys_a, keys_b, "a hit changed which fields exist");

        assert_eq!(a["hits"], 10);
        assert_eq!(b["hits"], 0);
        assert!(a["deviation"].as_f64().unwrap() > 0.0);
        assert!(
            b["deviation"].as_f64().unwrap() < 0.0,
            "the low tail is a finding too"
        );
    }

    /// FR-019. Nine further trials, every one of them a hit, and the claim does not move until the
    /// block closes on the tenth. The counts are the shipped ones: the view unlocks at ten and a
    /// block is ten, so the boundaries are 10, 20, 30.
    #[tokio::test]
    async fn the_reported_figures_advance_in_blocks() {
        let mut f = Fixture::new();
        let player = f.player();
        f.play(&player, 10, 1);

        let unlocked = json(call(&f.state, "/api/stats/me", Some(&player.token)).await).await;
        assert_eq!(unlocked["reported_trials"], 10);
        assert_eq!(unlocked["reported_hits"], 1);

        f.play(&player, 9, 9);
        let mid = json(call(&f.state, "/api/stats/me", Some(&player.token)).await).await;
        assert_eq!(mid["completed"], 19, "the record is reported live");
        assert_eq!(mid["hits"], 10);
        assert_eq!(mid["reported_trials"], 10, "the claim is not");
        assert_eq!(mid["deviation"], unlocked["deviation"]);
        assert_eq!(mid["wilson_lower"], unlocked["wilson_lower"]);

        f.play(&player, 1, 1);
        let closed = json(call(&f.state, "/api/stats/me", Some(&player.token)).await).await;
        assert_eq!(closed["reported_trials"], 20);
        assert_eq!(closed["reported_hits"], 11);
        assert!(closed["deviation"].as_f64().unwrap() > unlocked["deviation"].as_f64().unwrap());
    }

    /// FR-020 and D8's fourth measure: the aggregate counts the trials of accounts that never saw a
    /// statistics page, because conditioning the population on anything is what inflates it.
    #[tokio::test]
    async fn the_aggregate_counts_every_trial_by_every_account() {
        let mut f = Fixture::new();
        let seen = f.player();
        let unseen = f.player();
        f.play(&seen, 12, 3);
        f.play(&unseen, 3, 3);
        f.abandon(&unseen, 4);

        let body = json(call(&f.state, "/api/stats/aggregate", None).await).await;
        assert_eq!(body["trials"], 15, "the three-trial account is in the sum");
        assert_eq!(body["hits"], 6);
        assert_eq!(body["accounts"], 2);
        assert_eq!(body["abandoned"], 4);
        assert_eq!(body["expected_rate"], 0.125);
        assert_eq!(body["hit_rate"], 6.0 / 15.0);
    }

    /// FR-021 read strictly: a trial that was started and not answered is *not* abandoned until it
    /// can no longer be answered, so both sides of that cutoff are published. Without the open count
    /// a log younger than one lifetime reports zero abandonment while most of its starts sit
    /// unanswered, and the reader cannot see the difference between a third state and a bug.
    #[tokio::test]
    async fn unanswered_trials_are_open_until_their_lifetime_runs_out() {
        let mut f = Fixture::new();
        let player = f.player();
        f.play(&player, 5, 1);
        f.leave_open(&player, 7);
        f.abandon(&player, 2);

        let body = json(call(&f.state, "/api/stats/aggregate", None).await).await;
        assert_eq!(
            body["trials"], 5,
            "only answered trials are completed trials"
        );
        assert_eq!(body["open"], 7, "still answerable, so not given up on");
        assert_eq!(body["abandoned"], 2, "past the lifetime, so given up on");
        assert_eq!(
            body["trials"].as_u64().unwrap()
                + body["open"].as_u64().unwrap()
                + body["abandoned"].as_u64().unwrap(),
            14,
            "the three states must account for every trial that was started"
        );
    }

    /// FR-043 and SC-014. Both tails are reported, defined by the same distance, and the reader is
    /// told what the distance is.
    #[tokio::test]
    async fn both_tails_are_reported_side_by_side() {
        let mut f = Fixture::new();
        let high = f.player();
        let low = f.player();
        let middling = f.player();
        // Fifty trials, because the low tail costs more of them to earn than the high one: a
        // whole block of twenty-five misses is 1.9 sigma exactly, which is the edge and not
        // comfortably past it.
        f.play(&high, 50, 15);
        f.play(&low, 50, 0);
        f.play(&middling, 50, 6);

        let body = json(call(&f.state, "/api/stats/aggregate", None).await).await;
        assert_eq!(body["tail_high"], 1);
        assert_eq!(body["tail_low"], 1);
        assert_eq!(body["tail_sigma"], 1.9);
        assert_eq!(body["tail_min_trials"], 10);
    }

    /// The middle is published too, so a population that sits on chance is a visible measurement
    /// rather than two empty columns that read as a broken page. The account in the middle here is
    /// the one no pair of tail counts would ever have shown.
    #[tokio::test]
    async fn the_whole_spread_is_published_not_only_the_tails() {
        let mut f = Fixture::new();
        let high = f.player();
        let middling = f.player();
        f.play(&high, 50, 15);
        f.play(&middling, 50, 6);

        let body = json(call(&f.state, "/api/stats/aggregate", None).await).await;
        let bands = body["distribution"].as_array().expect("bands are an array");
        assert_eq!(body["qualified"], 2);
        assert_eq!(
            bands
                .iter()
                .map(|b| b["accounts"].as_u64().unwrap())
                .sum::<u64>(),
            2,
            "every qualified account is in exactly one band"
        );
        assert!(
            bands[0]["to"].as_f64().unwrap() < 0.0,
            "most negative first"
        );
        assert!(bands[0]["from"].is_null(), "the low end is open");
        assert!(
            bands.last().unwrap()["to"].is_null(),
            "so is the high end, however far out an account sits"
        );
        assert!(
            bands
                .iter()
                .any(|b| b["tail"] == false && b["accounts"] == 1),
            "the middling account shows up somewhere"
        );
    }

    /// The state the page will be in for a long time: trials played, nobody near either tail. The
    /// chart still has every column in it, and says how many accounts it is over.
    #[tokio::test]
    async fn a_population_on_chance_still_fills_the_chart() {
        let mut f = Fixture::new();
        let player = f.player();
        f.play(&player, 50, 6);

        let body = json(call(&f.state, "/api/stats/aggregate", None).await).await;
        assert_eq!(body["tail_high"], 0);
        assert_eq!(body["tail_low"], 0);
        assert_eq!(body["qualified"], 1);
        let bands = body["distribution"].as_array().unwrap();
        assert_eq!(
            bands.len(),
            11,
            "one column per rung, whatever the population"
        );
        assert_eq!(
            bands
                .iter()
                .map(|b| b["accounts"].as_u64().unwrap())
                .sum::<u64>(),
            1
        );
    }

    /// An account below the trial gate is in the aggregate's sums and not in the distribution: the
    /// bands are a statement about deviations, and a deviation over three trials is not one (D8).
    #[tokio::test]
    async fn the_distribution_is_gated_on_trials_like_the_statistics_view() {
        let mut f = Fixture::new();
        let short = f.player();
        f.play(&short, 3, 3);

        let body = json(call(&f.state, "/api/stats/aggregate", None).await).await;
        assert_eq!(body["trials"], 3, "the trials still count");
        assert_eq!(body["qualified"], 0);
        assert!(
            body["distribution"]
                .as_array()
                .unwrap()
                .iter()
                .all(|b| b["accounts"] == 0)
        );
    }

    #[tokio::test]
    async fn the_personal_view_is_closed_to_strangers() {
        let f = Fixture::new();
        let response = call(&f.state, "/api/stats/me", None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let public = call(&f.state, "/api/stats/aggregate", None).await;
        assert_eq!(public.status(), StatusCode::OK);
    }
}
