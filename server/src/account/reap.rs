//! Sweeping accounts that never reached the log (D32).
//!
//! `POST /api/account` writes a row for every name a visitor tries, and since D30 nothing rations
//! it. Most of those visitors never commit a trial, so left alone the table grows for ever with
//! rows carrying a name, a public identifier and a token hash — and no history at all. Over months
//! that is the whole of the growth in a database whose interesting half is the log.
//!
//! **One condition, and it is the only one that could be safe: the account appears nowhere in
//! `log_entry`.** An account the chain names stays for ever, because the chain is the product and
//! no entry in it is ever rewritten or removed (D2, D24). A trial that was committed and never
//! answered counts as appearing — it is published as abandoned (FR-021) and its account id is
//! inside the commit entry's hash — so a viewer who started one trial and walked away is kept, not
//! swept. The [`crate::db::Db::append_with`] discipline is untouched here: this deletes rows the
//! log does not mention and appends nothing.
//!
//! **Nothing published moves.** `GET /api/stats/aggregate` sums `log_entry` and counts accounts as
//! `COUNT(DISTINCT account_id)` over resolve rows, so an account with no trials contributes to none
//! of `trials`, `hits`, `deviation`, `accounts` or `abandoned`. That is D8's rule read backwards: a
//! figure that changed when an empty account went away would have been a figure with a population
//! condition hidden in it.
//!
//! What hangs off an account goes with it. `account_stats` gets a row of zeroes the first time a
//! holder opens the statistics page, `handoff_code` keeps a dead code until the next mint, and the
//! review queue of D25 is a filter over `account` itself — so a swept account leaves no name behind
//! for a human to approve, which is the ghost that would otherwise reach a reviewer.

use std::sync::atomic::{AtomicI64, Ordering};

use rusqlite::params;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::config::Config;
use crate::db::{Db, DbError};

/// How long between sweeps when the pass is driven from the request path.
///
/// An hour, because nothing here is urgent: the rows are inert and the only cost of holding them
/// one more hour is the space of one more hour's signups.
pub const SWEEP_AFTER_SECONDS: i64 = 60 * 60;

/// When *this process* last swept, as a Unix timestamp. Zero until the first sweep.
///
/// In process memory and not in a column, deliberately. A column means a second migration against
/// a live audit-log file, and [`crate::db::Db::migrate`] says what that costs; the content would be
/// bookkeeping for a job rather than anything a reader of the export needs. What it costs instead
/// is that the two processes of D24 sweep on their own clocks and a restart brings the next sweep
/// forward. Both are free, because the pass is idempotent and one that finds nothing is a single
/// anti-join.
static LAST_SWEEP_UNIX: AtomicI64 = AtomicI64::new(0);

/// The accounts a sweep takes: created before the cutoff and named nowhere in the log.
///
/// One string shared by all three statements below. A predicate that drifted between them would
/// delete the statistics row of an account that stays, and that account would then read as having
/// played nothing — a silent loss of somebody's record, from a job whose whole justification is
/// that it cannot lose anything.
const UNUSED: &str = "SELECT a.id FROM account a
                       WHERE a.created_at < ?1
                         AND NOT EXISTS (SELECT 1 FROM log_entry e WHERE e.account_id = a.id)";

/// Deletes every account past the grace period that the log does not mention, along with the rows
/// that hang off it. Returns how many accounts went.
///
/// Idempotent, and one transaction through [`Db::write`] — the other domain's process may be
/// appending, and this must queue on the same write lock as everything else rather than open its
/// own path to the file (D24, R9).
///
/// Children are deleted first because the schema has no `ON DELETE CASCADE`, and that absence is
/// worth keeping: with `foreign_keys = ON` a `DELETE FROM account` that would orphan a log entry
/// fails outright, so the constraint is a backstop for the predicate above rather than a formality.
/// Ordering the statements this way is what keeps the legitimate case from meeting it.
pub fn unused(db: &Db, cfg: &Config, now: &str) -> Result<u64, DbError> {
    let cutoff = created_before(now, cfg);
    db.write(|tx| {
        tx.execute(
            &format!("DELETE FROM account_stats WHERE account_id IN ({UNUSED})"),
            params![cutoff],
        )?;
        tx.execute(
            &format!("DELETE FROM handoff_code WHERE account_id IN ({UNUSED})"),
            params![cutoff],
        )?;
        let gone = tx.execute(
            &format!("DELETE FROM account WHERE id IN ({UNUSED})"),
            params![cutoff],
        )?;
        Ok(gone as u64)
    })
}

/// Runs a sweep if the last one has aged out, and does nothing otherwise.
///
/// **A stand-in for a timer, in the shape [`crate::stats::ranks::ensure_fresh`] already uses.** The
/// service has no scheduler yet — T102 is the task that brings one, for the rank pass — and rather
/// than invent a second mechanism beside it, this hangs off the one route that produces the rows it
/// removes: `POST /api/account`. That has a property the rank pass does not get from a read path.
/// The sweep is driven by exactly the traffic that creates the garbage, so a site nobody visits
/// runs no passes and needs none, and there is no way to make a public `GET` sweep the table.
///
/// When a scheduler lands, it calls [`unused`] directly and this goes with the call site.
pub fn ensure_swept(db: &Db, cfg: &Config, now: &str) -> Result<(), DbError> {
    if !take_turn(now) {
        return Ok(());
    }
    let gone = unused(db, cfg, now)?;
    if gone > 0 {
        // Per D28 this is a count and nothing per person: how many rows went, never which.
        tracing::info!(
            accounts = gone,
            "swept accounts that never reached the public log"
        );
    }
    Ok(())
}

/// Whether this call is the one that sweeps.
///
/// The timestamp is claimed **before** the pass runs, not after. Two concurrent creations in one
/// process therefore cannot both start one, and a sweep that fails waits for the next interval
/// instead of being retried by every request behind it — which on a broken database is the
/// difference between one error line and one per visitor.
fn take_turn(now: &str) -> bool {
    let now = unix(now);
    let last = LAST_SWEEP_UNIX.load(Ordering::Relaxed);
    if now.saturating_sub(last) < SWEEP_AFTER_SECONDS {
        return false;
    }
    LAST_SWEEP_UNIX
        .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
}

/// The instant an account must have been created before to be old enough to sweep.
///
/// Formatted the way the column holds it, so the comparison stays a string comparison — the same
/// property the log's timestamps are ordered by.
fn created_before(now: &str, cfg: &Config) -> String {
    let parsed = OffsetDateTime::parse(now, &Rfc3339).expect("a timestamp this process formatted");
    (parsed - time::Duration::hours(cfg.unused_account_grace_hours))
        .format(&Rfc3339)
        .expect("an RFC 3339 timestamp formats")
}

fn unix(at: &str) -> i64 {
    OffsetDateTime::parse(at, &Rfc3339)
        .expect("a timestamp this process formatted")
        .unix_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{self, handoff, name};
    use crate::log::chain::Body;
    use crate::stats::accumulate;

    /// When the accounts below were created, and when the sweep runs. Six months apart, so the
    /// grace period is comfortably behind whatever an operator has set it to.
    const SIGNED_UP: &str = "2026-01-10T09:00:00Z";
    const SWEEPS_AT: &str = "2026-07-10T09:00:00Z";

    fn open() -> (Db, Config) {
        (
            Db::open_in_memory().expect("an in-memory database opens"),
            Config::default(),
        )
    }

    fn signup(db: &Db, at: &str) -> String {
        account::create(db, "otherfren", at)
            .expect("the fixture name passes the filter")
            .id
    }

    /// A trial committed and never answered — the abandonment of FR-021, which is a record and not
    /// an absence.
    fn commit(db: &Db, account: &str, at: &str) {
        db.append(
            at,
            Body::Commit {
                trial: format!("trial-{account}"),
                account: account.to_string(),
                coordinate: "4821-9037".into(),
                commitment: "sha256:aa".into(),
                pool_version: 1,
                pool_manifest_hash: Some("sha256:pool".into()),
            },
        )
        .expect("the fixture writes a well-formed commit");
    }

    fn accounts(db: &Db) -> u32 {
        let r = db.reader().unwrap();
        r.query_row("SELECT COUNT(*) FROM account", [], |x| x.get(0))
            .unwrap()
    }

    fn count(db: &Db, sql: &str, account: &str) -> u32 {
        let r = db.reader().unwrap();
        r.query_row(sql, params![account], |x| x.get(0)).unwrap()
    }

    /// The promise the whole design rests on. The account's identifier is inside a hash in the
    /// chain, and the chain is never rewritten — so the row it names is kept however long ago the
    /// holder stopped playing, and however little else they did.
    #[test]
    fn an_account_the_log_names_is_never_swept() {
        let (db, cfg) = open();
        let played = signup(&db, SIGNED_UP);
        commit(&db, &played, SIGNED_UP);

        assert_eq!(unused(&db, &cfg, SWEEPS_AT).unwrap(), 0);
        assert_eq!(accounts(&db), 1);
        assert_eq!(db.verify_chain().unwrap(), 1, "the chain is untouched");
    }

    /// The holder who is looking at their coordinate right now. Nothing about them distinguishes
    /// them from a visitor who never came back except how long ago they signed up, which is why the
    /// grace period is the whole of the safety margin.
    #[test]
    fn an_account_inside_the_grace_period_is_left_alone() {
        let (db, cfg) = open();

        // One hour short of the grace period, measured with the same function the sweep measures by
        // — a literal here would be a second copy of the configured number and would go stale the
        // moment an operator moved it (D26).
        let cutoff = created_before(SWEEPS_AT, &cfg);
        let young = (OffsetDateTime::parse(&cutoff, &Rfc3339).unwrap() + time::Duration::hours(1))
            .format(&Rfc3339)
            .unwrap();
        signup(&db, &young);

        assert_eq!(
            unused(&db, &cfg, SWEEPS_AT).unwrap(),
            0,
            "an hour inside the grace period is inside it"
        );
        assert_eq!(accounts(&db), 1);

        // And it goes once the clock has moved past that hour, so the boundary is the grace period
        // and not something else.
        let later = (OffsetDateTime::parse(SWEEPS_AT, &Rfc3339).unwrap()
            + time::Duration::hours(2))
        .format(&Rfc3339)
        .unwrap();
        assert_eq!(unused(&db, &cfg, &later).unwrap(), 1);
        assert_eq!(accounts(&db), 0);
    }

    /// Everything that hangs off the row goes with it: the statistics row a visit to the statistics
    /// page created, the dead handoff code from a language switch, and the pending name that would
    /// otherwise sit in a reviewer's queue belonging to nobody.
    #[test]
    fn a_swept_account_leaves_nothing_hanging_off_it() {
        let (db, cfg) = open();
        let idler = signup(&db, SIGNED_UP);

        // The row of zeroes a holder gets by opening the statistics page before playing.
        accumulate::ensure_current(&db, &cfg, &idler, SIGNED_UP).unwrap();
        handoff::mint(&db, &idler, SIGNED_UP).unwrap();
        assert_eq!(
            name::pending(&db, 10).unwrap().len(),
            1,
            "the name is in the review queue"
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM account_stats WHERE account_id = ?1",
                &idler
            ),
            1
        );

        assert_eq!(unused(&db, &cfg, SWEEPS_AT).unwrap(), 1);

        assert_eq!(accounts(&db), 0);
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM account_stats WHERE account_id = ?1",
                &idler
            ),
            0
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM handoff_code WHERE account_id = ?1",
                &idler
            ),
            0
        );
        assert!(
            name::pending(&db, 10).unwrap().is_empty(),
            "a reviewer must not be shown a name nobody holds"
        );
    }

    /// Two processes run this against one file (D24), so a second pass over the same state has to
    /// be a no-op rather than an error.
    #[test]
    fn a_second_sweep_finds_nothing() {
        let (db, cfg) = open();
        signup(&db, SIGNED_UP);
        assert_eq!(unused(&db, &cfg, SWEEPS_AT).unwrap(), 1);
        assert_eq!(unused(&db, &cfg, SWEEPS_AT).unwrap(), 0);
        assert_eq!(unused(&db, &cfg, SWEEPS_AT).unwrap(), 0);
    }

    /// The backstop, asserted so that adding `ON DELETE CASCADE` to the schema has to break a test
    /// that says why not. If the predicate above is ever wrong, this is what stops the mistake from
    /// leaving a log entry pointing at an account that no longer exists.
    #[test]
    fn the_schema_refuses_to_delete_an_account_the_log_names() {
        let (db, _cfg) = open();
        let played = signup(&db, SIGNED_UP);
        commit(&db, &played, SIGNED_UP);

        let forced = db.write(|tx| {
            tx.execute("DELETE FROM account WHERE id = ?1", params![played])?;
            Ok(())
        });
        assert!(
            forced.is_err(),
            "the foreign key from log_entry has to refuse this"
        );
        assert_eq!(accounts(&db), 1);
        assert_eq!(db.verify_chain().unwrap(), 1);
    }

    /// One sweep per interval, whatever the traffic. The gate is what keeps a busy signup hour from
    /// running the anti-join once per visitor.
    #[test]
    fn the_interval_gate_lets_one_pass_through_per_period() {
        let (db, cfg) = open();
        signup(&db, SIGNED_UP);

        ensure_swept(&db, &cfg, SWEEPS_AT).unwrap();
        assert_eq!(accounts(&db), 0, "the first call takes the turn");

        signup(&db, SIGNED_UP);
        ensure_swept(&db, &cfg, SWEEPS_AT).unwrap();
        assert_eq!(
            accounts(&db),
            1,
            "the second call is inside the interval and does nothing"
        );
    }

    /// D8's rule, checked rather than argued: the aggregate is over every trial by every account,
    /// and an account that contributed no trials contributed nothing to take away. Every figure the
    /// statistics page publishes has to be identical either side of the sweep.
    #[tokio::test]
    async fn sweeping_does_not_move_the_published_aggregate() {
        use crate::http::routes::stats::test_support::{Fixture, json};
        use axum::body::Body as AxumBody;
        use axum::http::Request;
        use tower::ServiceExt;

        async fn aggregate(state: &crate::http::AppState) -> serde_json::Value {
            let request = Request::builder()
                .method("GET")
                .uri("/api/stats/aggregate")
                .body(AxumBody::empty())
                .unwrap();
            json(
                crate::http::router(state.clone())
                    .oneshot(request)
                    .await
                    .unwrap(),
            )
            .await
        }

        let mut f = Fixture::new();
        let player = f.player();
        // Two accounts that will be swept: one that only ever looked at its statistics page, one
        // that did nothing at all.
        let looked = f.player();
        f.player();
        f.play(&player, 12, 3);
        f.abandon(&player, 2);
        accumulate::ensure_current(&f.state.db, &f.state.config, &looked.id, SIGNED_UP).unwrap();

        let before = aggregate(&f.state).await;

        // The fixture signs its players up on 2026-07-01, so a sweep in 2027 is well past any grace
        // period an operator would set.
        let gone = unused(&f.state.db, &f.state.config, "2027-01-01T00:00:00Z").unwrap();
        assert_eq!(gone, 2, "both idle accounts go and the player stays");

        let after = aggregate(&f.state).await;
        for figure in [
            "trials",
            "hits",
            "hit_rate",
            "deviation",
            "abandoned",
            "accounts",
            "qualified",
            "tail_high",
            "tail_low",
        ] {
            assert_eq!(before[figure], after[figure], "the sweep moved {figure}");
        }
        assert_eq!(before["trials"], 12);
        assert_eq!(
            before["accounts"], 1,
            "the account count is read off the log, so the idle rows were never in it"
        );
        assert_eq!(before["abandoned"], 2);
    }
}
