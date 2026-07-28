//! Work the process does on a clock rather than on a request.
//!
//! Two of these, and both are small on purpose. A background task in a service whose product is an
//! append-only record is a writer nobody asked for, so the rule they follow is that they may only
//! do work a request would otherwise have paid for — never work that would not have happened at
//! all.

use std::sync::Arc;
use std::time::Duration;

use crate::config::Config;
use crate::db::{Db, now_rfc3339};
use crate::metrics::Metrics;
use crate::stats::ranks;

/// How often the traffic counters are written down.
///
/// A minute. Long enough that a busy page is one write rather than hundreds, short enough that a
/// crash costs a minute of a figure that is a rough sense of traffic and nothing anybody decides
/// on. The unique-visitor set is not affected by the interval — it is written as a maximum, so a
/// flush never double-counts.
const FLUSH_AFTER: Duration = Duration::from_secs(60);

/// The fifteen-minute rank pass of D23 (T102).
///
/// Ranks are materialised, and something has to materialise them. Before this existed the only
/// trigger was a reader arriving — which meant the first visitor after a quiet night paid for the
/// pass, and a board nobody looked at published a `ranks_updated_at` from whenever the last person
/// happened to load it. A timer makes the figure mean what the page says it means.
///
/// **Not the only trigger.** [`ranks::ensure_fresh`] stays on the read path, gated on the same
/// interval, so it is a no-op whenever this task is keeping up and a self-heal when it is not — a
/// process whose timer died still serves fresh ranks rather than serving stale ones silently.
///
/// **Two processes share the database** (D24), so both run this. That is not a race worth
/// preventing: the pass is idempotent, it is gated on `ranked_at` inside a write transaction, and
/// the loser of a tie finds the work already done and returns.
///
/// The pass is a blocking SQLite write, so it runs on the blocking pool and never on the runtime
/// thread that is answering requests.
pub fn spawn_rank_timer(db: Arc<Db>, config: Arc<Config>) -> tokio::task::JoinHandle<()> {
    let period = Duration::from_secs(ranks::RECOMPUTE_AFTER_SECONDS as u64);
    tokio::spawn(async move {
        // Skips missed ticks rather than firing a burst of them. A process suspended for an hour
        // owes one pass, not four.
        let mut ticker = tokio::time::interval(period);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick is immediate: a process that has just restarted after a deploy should
        // publish ranks it computed now, not ranks whoever loaded the board last computed.
        loop {
            ticker.tick().await;
            let (db, config) = (Arc::clone(&db), Arc::clone(&config));
            let outcome = tokio::task::spawn_blocking(move || {
                ranks::ensure_fresh(&db, &config, &now_rfc3339())
            })
            .await;
            match outcome {
                Ok(Ok(())) => {}
                // Logged and not fatal. A failed pass leaves the previous ranks standing, which is
                // stale but true; taking the process down over it would take the trial with it.
                Ok(Err(e)) => tracing::error!(error = %e, "rank recomputation failed"),
                Err(e) => tracing::error!(error = %e, "rank recomputation panicked"),
            }
        }
    })
}

/// Writes the traffic counters down (FR-052, D28, T112).
///
/// The counting itself happens in memory on the request path; this is the only thing that touches
/// the database on their behalf, which is what keeps a page view from queueing behind the append
/// that writes the audit log.
///
/// A failed flush is logged and the counts are gone — they were drained before the write. That is
/// the right way round for this table and would be the wrong way round for anything else here: a
/// retry queue for page-view counts is more machinery than the figure is worth, and the figure it
/// would protect is "roughly how many people came".
pub fn spawn_metrics_flush(db: Arc<Db>, metrics: Arc<Metrics>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(FLUSH_AFTER);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The immediate first tick lands on an empty tally and writes nothing.
        loop {
            ticker.tick().await;
            let (db, metrics) = (Arc::clone(&db), Arc::clone(&metrics));
            let outcome =
                tokio::task::spawn_blocking(move || metrics.flush(&db, &now_rfc3339())).await;
            match outcome {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::error!(error = %e, "traffic counters were not written"),
                Err(e) => tracing::error!(error = %e, "metrics flush panicked"),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Locale;
    use crate::http::routes::stats::test_support::Fixture;
    use crate::metrics::name;
    use crate::stats::ranks::last_computed;

    fn computed(db: &Db) -> Option<String> {
        last_computed(&db.reader().unwrap()).unwrap()
    }

    /// The point of the task: a rank exists because time passed, not because somebody loaded the
    /// board. Before T102 this assertion could only be made to hold by issuing a `GET`.
    #[tokio::test]
    async fn the_timer_computes_ranks_with_no_reader() {
        // The shipped floor is a hundred trials; lowered so an eligible account can be built
        // without writing a hundred entries. The three distinct days it also needs stand.
        let mut config = Config::default();
        config.thresholds.eligibility_trials = config.thresholds.stats_unlock_at;
        let mut fixture = Fixture::with_config(config);
        let player = fixture.player();
        fixture.play_across_days(&player, 4, 3, 3);
        let state = fixture.state.clone();

        assert_eq!(computed(&state.db), None, "no pass has run");

        let handle = spawn_rank_timer(Arc::clone(&state.db), Arc::clone(&state.config));
        for _ in 0..200 {
            if computed(&state.db).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        handle.abort();

        assert!(
            computed(&state.db).is_some(),
            "a pass ran without a request reaching the process"
        );
    }

    /// The counters reach the table without any request having asked them to, which is the whole
    /// job: the counting is in memory and something has to write it down.
    #[tokio::test]
    async fn the_flush_writes_what_was_counted() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let metrics = Arc::new(Metrics::new(Locale::De, &now_rfc3339()));
        metrics.count(name::PAGE_VIEW, &now_rfc3339());

        let handle = spawn_metrics_flush(Arc::clone(&db), Arc::clone(&metrics));
        let mut written = 0i64;
        for _ in 0..200 {
            written = db
                .reader()
                .unwrap()
                .query_row(
                    "SELECT COALESCE(SUM(count), 0) FROM daily_metric WHERE metric = ?1",
                    [name::PAGE_VIEW],
                    |r| r.get(0),
                )
                .unwrap();
            if written > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        handle.abort();
        assert_eq!(written, 1);
    }
}
