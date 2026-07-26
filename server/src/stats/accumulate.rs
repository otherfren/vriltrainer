//! `account_stats`, maintained on resolve rather than computed per request.
//!
//! The table is a **cache of the log**, never a second source of truth. D2 makes the published
//! record the authority and SC-012 lets a stranger recompute every figure here from the export, so
//! anything this module stores has to be reproducible from `log_entry` alone — which is what
//! [`rebuild`] does, and why it exists rather than being a repair script somebody writes after the
//! first restore from backup.

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::config::Config;
use crate::db::{Db, DbError, utc_day};
use crate::stats::{AccountStats, blocks, measures};

/// Folds one resolved trial into the account's totals.
///
/// Designed to be passed to [`crate::db::Db::append_with`], so the figures and the log entry they
/// describe commit in the same transaction. Anything else allows a state where the record says one
/// thing and the leaderboard says another, and the record is the one that is published.
///
/// The whole [`Config`] rather than the three numbers it reads: every one of them is a threshold
/// D26 expects an operator to move, and a signature that spelled them out would be edited each time
/// one more of them turned out to matter here.
pub fn on_resolve(
    tx: &Transaction<'_>,
    cfg: &Config,
    account_id: &str,
    hit: bool,
    at: &str,
) -> Result<(), DbError> {
    let day = utc_day(at);

    // `distinct_utc_days` needs no side table: a resolve always happens now, so its day is either
    // the one already counted or a new one. `IS NOT` rather than `<>` because the first resolve
    // finds `last_utc_day` null, and `NULL <> '2026-07-25'` is null, not true.
    let (completed, hits): (u64, u64) = tx.query_row(
        "INSERT INTO account_stats
             (account_id, completed, hits, abandoned, distinct_utc_days, last_utc_day,
              wilson_lower, wilson_upper, deviation, eligible, updated_at)
         VALUES (?1, 1, ?2, 0, 1, ?3, 0, 1, 0, 0, ?4)
         ON CONFLICT (account_id) DO UPDATE SET
             completed         = completed + 1,
             hits              = hits + ?2,
             distinct_utc_days = distinct_utc_days + (last_utc_day IS NOT ?3),
             last_utc_day      = ?3,
             updated_at        = ?4
         RETURNING completed, hits",
        params![account_id, hit as i64, day, at],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let advanced = at_boundary(completed, hits, cfg);
    let abandoned = recount_abandoned(tx, account_id, &still_open_from(at, cfg))?;
    store_measures(tx, account_id, abandoned, advanced)
}

/// Counts a trial the account never completed. Abandonment is the absence of a resolve entry, so
/// this is a convenience for the statistics page and never a source of truth — the export is
/// (FR-027, SC-012).
///
/// `still_open_from` is the cutoff a trial has to have been committed at or after to be merely
/// *open* rather than abandoned. It is the same string [`crate::http::limits::open_trials`] counts
/// the other side of, so every commit is on exactly one side: two rules that disagreed here would
/// produce a trial that both blocks the concurrency cap and counts as given up on.
///
/// Takes a [`Connection`] rather than a [`Transaction`] so the statistics page can call it on a
/// pooled reader. A `GET` that took the write lock would queue behind the other domain's appends
/// for the sake of a number it is only going to print.
pub fn recount_abandoned(
    conn: &Connection,
    account_id: &str,
    still_open_from: &str,
) -> Result<u64, DbError> {
    let count = conn.query_row(
        "SELECT COUNT(*) FROM log_entry c
          WHERE c.kind = 'commit' AND c.account_id = ?1 AND c.at < ?2
            AND NOT EXISTS (SELECT 1 FROM log_entry r
                             WHERE r.trial_id = c.trial_id AND r.kind = 'resolve')",
        params![account_id, still_open_from],
        |r| r.get(0),
    )?;
    Ok(count)
}

/// Recomputes one account's row from the log, replaying its resolves in sequence order.
///
/// Every column here is derived, so this is what makes the table droppable: after a restore, a
/// schema change, or a resolve path that forgot to call [`on_resolve`], the log still holds the
/// answer and this recovers it. It also settles which of the two is authoritative when they
/// disagree — the log, always.
pub fn rebuild(
    tx: &Transaction<'_>,
    cfg: &Config,
    account_id: &str,
    now: &str,
) -> Result<(), DbError> {
    let mut stmt = tx.prepare(
        "SELECT at, hit FROM log_entry
          WHERE account_id = ?1 AND kind = 'resolve' ORDER BY seq",
    )?;
    let mut rows = stmt.query(params![account_id])?;

    let mut completed = 0u64;
    let mut hits = 0u64;
    let mut days = 0u32;
    let mut last_day: Option<String> = None;
    let mut measures = Measures::NOTHING_KNOWN;

    while let Some(row) = rows.next()? {
        let at: String = row.get(0)?;
        let hit: i64 = row.get(1)?;
        completed += 1;
        hits += (hit != 0) as u64;

        // Sequence order is time order — the chain is appended to under one write lock — so a day
        // that differs from the previous one has not been seen before.
        let day = utc_day(&at);
        if last_day.as_deref() != Some(day) {
            days += 1;
            last_day = Some(day.to_string());
        }
        if let Some(advanced) = at_boundary(completed, hits, cfg) {
            measures = advanced;
        }
    }
    drop(rows);
    drop(stmt);

    let abandoned = recount_abandoned(tx, account_id, &still_open_from(now, cfg))?;
    tx.execute(
        "INSERT INTO account_stats
             (account_id, completed, hits, abandoned, distinct_utc_days, last_utc_day,
              wilson_lower, wilson_upper, deviation, eligible, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10)
         ON CONFLICT (account_id) DO UPDATE SET
             completed         = excluded.completed,
             hits              = excluded.hits,
             abandoned         = excluded.abandoned,
             distinct_utc_days = excluded.distinct_utc_days,
             last_utc_day      = excluded.last_utc_day,
             wilson_lower      = excluded.wilson_lower,
             wilson_upper      = excluded.wilson_upper,
             deviation         = excluded.deviation,
             updated_at        = excluded.updated_at",
        params![
            account_id,
            completed,
            hits,
            abandoned,
            days,
            last_day,
            measures.lower,
            measures.upper,
            measures.deviation,
            now
        ],
    )?;
    Ok(())
}

/// Brings one account's row level with the log if it has fallen behind, and does nothing if it has
/// not.
///
/// One indexed `COUNT` on the read path buys the guarantee that the statistics page shows the
/// account's own last trial. Without it a resolve path that does not call [`on_resolve`] shows a
/// user zeroes after a hundred trials, and the first thing they would do is stop trusting the site.
pub fn ensure_current(db: &Db, cfg: &Config, account_id: &str, now: &str) -> Result<(), DbError> {
    let behind = {
        let reader = db.reader()?;
        is_behind(&reader, account_id)?
    };
    if behind {
        db.write(|tx| rebuild(tx, cfg, account_id, now))?;
    }
    Ok(())
}

/// The same repair across every account that needs it, for the passes that read the whole table.
///
/// Returns how many rows were rebuilt, which is zero on a healthy database and therefore worth
/// logging when it is not.
pub fn refresh_stale(db: &Db, cfg: &Config, now: &str) -> Result<u32, DbError> {
    let behind: Vec<String> = {
        let reader = db.reader()?;
        let mut stmt = reader.prepare(
            "SELECT played.account_id
               FROM (SELECT account_id, COUNT(*) AS n FROM log_entry
                      WHERE kind = 'resolve' GROUP BY account_id) AS played
               LEFT JOIN account_stats s ON s.account_id = played.account_id
              WHERE COALESCE(s.completed, -1) <> played.n",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<Vec<String>>>()?
    };
    if behind.is_empty() {
        return Ok(0);
    }
    tracing::info!(
        accounts = behind.len(),
        "rebuilding account statistics from the log"
    );
    db.write(|tx| {
        for account_id in &behind {
            rebuild(tx, cfg, account_id, now)?;
        }
        Ok(())
    })?;
    Ok(behind.len() as u32)
}

/// One account's stored figures, or a row of zeroes for an account that has completed nothing.
///
/// Zeroes rather than an error: an account with no trials is the ordinary case for a visitor who
/// has just signed up, and the statistics page has to answer them too (with the locked form).
pub fn load(conn: &Connection, account_id: &str) -> Result<AccountStats, DbError> {
    let found = conn
        .query_row(
            "SELECT completed, hits, abandoned, distinct_utc_days, wilson_lower, wilson_upper,
                    deviation, eligible, rank_slug
               FROM account_stats WHERE account_id = ?1",
            params![account_id],
            |r| {
                Ok(AccountStats {
                    completed: r.get(0)?,
                    hits: r.get(1)?,
                    abandoned: r.get(2)?,
                    distinct_utc_days: r.get(3)?,
                    wilson_lower: r.get(4)?,
                    wilson_upper: r.get(5)?,
                    deviation: r.get(6)?,
                    eligible: r.get::<_, i64>(7)? != 0,
                    rank: r.get(8)?,
                })
            },
        )
        .optional()?;
    Ok(found.unwrap_or(AccountStats {
        completed: 0,
        hits: 0,
        abandoned: 0,
        distinct_utc_days: 0,
        wilson_lower: Measures::NOTHING_KNOWN.lower,
        wilson_upper: Measures::NOTHING_KNOWN.upper,
        deviation: Measures::NOTHING_KNOWN.deviation,
        eligible: false,
        rank: None,
    }))
}

/// Hits among the account's **first** `n` completed trials, in log order.
///
/// The stored measures were computed at a block boundary over exactly this prefix (FR-019), so this
/// is how the page prints the pair they stand over. Taking the last `n` instead would let a user
/// drop their worst opening block by playing on, which is optional stopping wearing a hat.
pub fn hits_within(conn: &Connection, account_id: &str, n: u64) -> Result<u64, DbError> {
    if n == 0 {
        return Ok(0);
    }
    let hits = conn.query_row(
        "SELECT COALESCE(SUM(hit), 0) FROM
             (SELECT hit FROM log_entry
               WHERE account_id = ?1 AND kind = 'resolve' ORDER BY seq LIMIT ?2)",
        params![account_id, n],
        |r| r.get(0),
    )?;
    Ok(hits)
}

/// The instant before which an unresolved commit is permanently abandoned rather than still open.
///
/// One clock for a trial's life, running from its commit — the same rule the answer path expires by
/// and the concurrency cap counts by.
pub fn still_open_from(now: &str, cfg: &Config) -> String {
    let parsed = OffsetDateTime::parse(now, &Rfc3339).expect("a timestamp this process formatted");
    (parsed - time::Duration::hours(cfg.trial_lifetime_hours))
        .format(&Rfc3339)
        .expect("an RFC 3339 timestamp formats")
}

/// The three figures that advance together, kept in one value so a caller cannot write two of
/// them and forget the third — which is what a bare tuple invited when the upper bound was added.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Measures {
    lower: f64,
    upper: f64,
    deviation: f64,
}

impl Measures {
    /// What an account with nothing behind it is worth: no guaranteed floor, no ruled-out ceiling,
    /// no distance from chance.
    const NOTHING_KNOWN: Self = Self {
        lower: 0.0,
        upper: 1.0,
        deviation: 0.0,
    };
}

/// The measures, but only at a block boundary — `None` anywhere else, which is what holds the
/// figure still between boundaries (FR-019, D17).
fn at_boundary(completed: u64, hits: u64, cfg: &Config) -> Option<Measures> {
    blocks::is_boundary(
        completed,
        cfg.block_size as u64,
        cfg.thresholds.stats_unlock_at as u64,
    )
    .then(|| Measures {
        lower: measures::wilson_lower(hits, completed, measures::WILSON_Z),
        upper: measures::wilson_upper(hits, completed, measures::WILSON_Z),
        deviation: measures::deviation(hits, completed),
    })
}

/// Writes the abandoned count always and the measures only when the block advanced.
fn store_measures(
    tx: &Transaction<'_>,
    account_id: &str,
    abandoned: u64,
    advanced: Option<Measures>,
) -> Result<(), DbError> {
    tx.execute(
        "UPDATE account_stats
            SET abandoned    = ?2,
                wilson_lower = COALESCE(?3, wilson_lower),
                wilson_upper = COALESCE(?4, wilson_upper),
                deviation    = COALESCE(?5, deviation)
          WHERE account_id = ?1",
        params![
            account_id,
            abandoned,
            advanced.map(|m| m.lower),
            advanced.map(|m| m.upper),
            advanced.map(|m| m.deviation)
        ],
    )?;
    Ok(())
}

/// Whether the stored completed count disagrees with the log.
///
/// Only `completed` is checked. It is the one column that can only change when a resolve is
/// appended, so it is the cheap witness for every other column moving with it — `abandoned` is
/// deliberately not part of this, because it changes with the clock rather than with a write and
/// the read path recounts it anyway.
fn is_behind(conn: &Connection, account_id: &str) -> Result<bool, DbError> {
    let (stored, logged): (i64, i64) = conn.query_row(
        "SELECT COALESCE((SELECT completed FROM account_stats WHERE account_id = ?1), -1),
                (SELECT COUNT(*) FROM log_entry WHERE account_id = ?1 AND kind = 'resolve')",
        params![account_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(stored != logged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::chain::Body;

    /// A day of trials for one account, written straight to the log the way the answer path does.
    struct Player {
        db: Db,
        cfg: Config,
        account: String,
        next: u32,
    }

    impl Player {
        fn new() -> Self {
            let db = Db::open_in_memory().expect("an in-memory database opens");
            let account = crate::account::create(&db, "otherfren", "2026-07-01T00:00:00Z")
                .expect("the fixture name passes the filter")
                .id;
            Player {
                db,
                cfg: Config::default(),
                account,
                next: 0,
            }
        }

        /// Commits and resolves one trial at `at`, folding it in exactly as the answer path would.
        fn play(&mut self, hit: bool, at: &str) {
            let trial = self.commit(at);
            let cfg = self.cfg.clone();
            let account = self.account.clone();
            self.db
                .append_with(
                    at,
                    Body::Resolve {
                        trial,
                        chosen: "img_1".into(),
                        target: if hit { "img_1".into() } else { "img_2".into() },
                        hit,
                        s_server: "aa".into(),
                        s_client: "bb".into(),
                        nonce: "cc".into(),
                    },
                    |tx, entry| on_resolve(tx, &cfg, &account, hit, &entry.at),
                )
                .unwrap();
        }

        /// A trial started and never answered.
        fn commit(&mut self, at: &str) -> String {
            self.next += 1;
            let trial = format!("t{}", self.next);
            self.db
                .append(
                    at,
                    Body::Commit {
                        trial: trial.clone(),
                        account: self.account.clone(),
                        coordinate: "4821-9037".into(),
                        commitment: "sha256:aa".into(),
                        pool_version: 1,
                    },
                )
                .unwrap();
            trial
        }

        fn stats(&self) -> AccountStats {
            let r = self.db.reader().unwrap();
            load(&r, &self.account).unwrap()
        }
    }

    const DAY_ONE: &str = "2026-07-20T09:00:00Z";
    const DAY_TWO: &str = "2026-07-21T09:00:00Z";
    const DAY_THREE: &str = "2026-07-22T09:00:00Z";

    #[test]
    fn resolves_accumulate_into_the_account_row() {
        let mut p = Player::new();
        for i in 0..10 {
            p.play(i % 4 == 0, DAY_ONE);
        }
        let s = p.stats();
        assert_eq!((s.completed, s.hits), (10, 3));
        assert_eq!(s.distinct_utc_days, 1);
    }

    /// R4: distinct UTC calendar days, which is the only part of the eligibility rule a farm
    /// cannot compress by running more accounts at once (D21).
    #[test]
    fn a_day_is_counted_once_however_many_trials_it_holds() {
        let mut p = Player::new();
        for day in [DAY_ONE, DAY_ONE, DAY_TWO, DAY_TWO, DAY_TWO, DAY_THREE] {
            p.play(false, day);
        }
        assert_eq!(p.stats().distinct_utc_days, 3);
    }

    /// FR-019. The figure that constitutes a claim may only move at a boundary; the record of what
    /// happened moves with every trial.
    #[test]
    fn the_measures_stand_still_between_blocks() {
        let mut p = Player::new();
        let unlock = p.cfg.thresholds.stats_unlock_at as u64;

        for _ in 0..unlock {
            p.play(true, DAY_ONE);
        }
        let unlocked = p.stats();
        assert!(unlocked.deviation > 0.0, "the unlock is the first boundary");

        // The next boundary above the unlock, computed rather than assumed: the block may be
        // smaller than the unlock (it is, at ten and ten), and an expression that subtracts one
        // from the other underflows the moment an operator moves either (D26).
        let block = p.cfg.block_size as u64;
        let next = blocks::reported_trials(unlock + block, block).max(unlock + 1);

        // Every trial from here to that boundary is a miss, and the claim must not notice.
        for _ in 0..(next - unlock - 1) {
            p.play(false, DAY_ONE);
        }
        let mid_block = p.stats();
        assert_eq!(mid_block.deviation, unlocked.deviation);
        assert_eq!(mid_block.wilson_lower, unlocked.wilson_lower);
        assert_eq!(mid_block.completed, next - 1);
        assert_eq!(
            mid_block.hits, unlocked.hits,
            "the record itself is reported live"
        );

        p.play(false, DAY_ONE);
        let boundary = p.stats();
        assert!(
            boundary.deviation < unlocked.deviation,
            "the block closed on nothing but misses and the figure did not move"
        );
    }

    /// FR-021 and SC-012: a trial that can no longer be answered is abandoned, and one that still
    /// can is not. The two counts partition the account's commits, with no trial in both or
    /// neither.
    #[test]
    fn abandonment_is_counted_from_the_same_clock_the_trial_expires_on() {
        let mut p = Player::new();
        p.commit("2026-07-01T09:00:00Z"); // long past its lifetime
        p.commit(DAY_THREE); // still open at DAY_THREE
        p.play(true, DAY_THREE);

        let r = p.db.reader().unwrap();
        let cutoff = still_open_from(DAY_THREE, &p.cfg);
        assert_eq!(recount_abandoned(&r, &p.account, &cutoff).unwrap(), 1);
        drop(r);
        assert_eq!(p.stats().abandoned, 1);
    }

    /// The property that makes the table droppable: whatever the incremental path did, replaying
    /// the log has to land on the same row. If these ever diverge, the log wins.
    #[test]
    fn replaying_the_log_reproduces_the_incremental_row() {
        let mut p = Player::new();
        for i in 0..60 {
            p.play(i % 3 == 0, if i < 30 { DAY_ONE } else { DAY_TWO });
        }
        let incremental = p.stats();

        let cfg = p.cfg.clone();
        let account = p.account.clone();
        p.db.write(|tx| rebuild(tx, &cfg, &account, DAY_TWO))
            .unwrap();

        assert_eq!(p.stats(), incremental);
    }

    /// The case this repair exists for: a log full of resolves and no statistics row at all.
    #[test]
    fn a_missing_row_is_rebuilt_from_the_log() {
        let mut p = Player::new();
        for i in 0..30 {
            p.play(i % 8 == 0, DAY_ONE);
        }
        let expected = p.stats();

        p.db.write(|tx| {
            tx.execute("DELETE FROM account_stats", [])?;
            Ok(())
        })
        .unwrap();
        assert_eq!(p.stats().completed, 0);

        let cfg = p.cfg.clone();
        assert_eq!(refresh_stale(&p.db, &cfg, DAY_ONE).unwrap(), 1);
        assert_eq!(p.stats(), expected);
        assert_eq!(
            refresh_stale(&p.db, &cfg, DAY_ONE).unwrap(),
            0,
            "a healthy database rebuilds nothing"
        );
    }

    /// The stored measures were computed over the first `n` trials, so the hits printed beside them
    /// have to come from the same prefix.
    #[test]
    fn the_reported_hits_come_from_the_opening_trials() {
        let mut p = Player::new();
        for _ in 0..10 {
            p.play(true, DAY_ONE);
        }
        for _ in 0..10 {
            p.play(false, DAY_ONE);
        }
        let r = p.db.reader().unwrap();
        assert_eq!(hits_within(&r, &p.account, 10).unwrap(), 10);
        assert_eq!(hits_within(&r, &p.account, 20).unwrap(), 10);
        assert_eq!(hits_within(&r, &p.account, 0).unwrap(), 0);
    }
}
