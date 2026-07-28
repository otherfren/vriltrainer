//! Traffic figures (FR-052, D28, T112–T114).
//!
//! The operator needs to know whether anybody is coming, and the site's whole argument is that it
//! does not keep anything about who. Those two are compatible only if the counting happens without
//! a per-visitor record ever existing — so this counts in memory, writes integers, and keeps
//! nothing else.
//!
//! What is stored is one row per day, per locale, per named event, holding a number. No path, no
//! address, no session, no identifier of any kind. There is no analytics vendor, no cookie and no
//! pixel; the counters are incremented by the handlers themselves.
//!
//! # Unique visitors
//!
//! The one figure that needs state. A salt is drawn at midnight and never written down; an address
//! is hashed with it, truncated, and kept in a set; at rollover the size of the set is written and
//! the salt and the set are dropped (T113). Yesterday's hashes therefore cannot be recomputed even
//! by the process that made them, and a restart loses the day's set — which undercounts, and is the
//! trade this design is choosing on purpose.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Mutex;

use rand::Rng;
use rusqlite::params;
use sha2::{Digest, Sha256};

use crate::config::Locale;
use crate::db::{Db, DbError};

/// The named events. String constants rather than an enum on the wire, because they are also the
/// column values an operator reads in `metrics --since` and greps for in an archive.
pub mod name {
    pub const PAGE_VIEW: &str = "page_view";
    pub const UNIQUE_VISITORS: &str = "unique_visitors";
    pub const ACCOUNT_CREATED: &str = "account_created";
    pub const TRIAL_STARTED: &str = "trial_started";
    pub const TRIAL_COMPLETED: &str = "trial_completed";
    pub const NAME_SUBMITTED: &str = "name_submitted";
    pub const NAME_APPROVED: &str = "name_approved";
    pub const PROOF_OPENED: &str = "proof_opened";
    pub const LOG_DOWNLOAD: &str = "log_download";

    /// Every metric this process knows how to count, for the reader that has to print zeroes for
    /// the ones nothing happened on.
    pub const ALL: &[&str] = &[
        PAGE_VIEW,
        UNIQUE_VISITORS,
        ACCOUNT_CREATED,
        TRIAL_STARTED,
        TRIAL_COMPLETED,
        NAME_SUBMITTED,
        NAME_APPROVED,
        PROOF_OPENED,
        LOG_DOWNLOAD,
    ];
}

/// The in-process counters. One of these lives in [`crate::http::AppState`].
///
/// Counting in memory and flushing on a timer rather than writing per event: every event here is a
/// request, and a database write per request would put page views in contention with the append
/// that writes the audit log. A flush that is lost to a crash costs a few counts of a figure whose
/// entire purpose is a rough sense of traffic.
pub struct Metrics {
    locale: Locale,
    state: Mutex<Counters>,
}

struct Counters {
    /// `YYYY-MM-DD` in UTC, the day the counts below belong to.
    day: String,
    counts: HashMap<&'static str, u64>,
    /// Redrawn at every rollover and never persisted.
    salt: [u8; 16],
    /// Truncated hashes of the addresses seen today.
    seen: HashSet<[u8; 8]>,
}

impl Metrics {
    pub fn new(locale: Locale, today: &str) -> Self {
        Metrics {
            locale,
            state: Mutex::new(Counters {
                day: day_of(today),
                counts: HashMap::new(),
                salt: rand::rng().random(),
                seen: HashSet::new(),
            }),
        }
    }

    /// Counts one event. `now` is an RFC 3339 instant; only its date is used.
    pub fn count(&self, metric: &'static str, now: &str) {
        let mut state = self.state.lock().expect("the counters are not poisoned");
        state.roll(&day_of(now));
        *state.counts.entry(metric).or_default() += 1;
    }

    /// Records that this address has been seen today, without keeping the address.
    ///
    /// The hash is over the salt *and* the address, so the same visitor tomorrow is a different
    /// entry and yesterday's entries cannot be re-derived from a leaked address list — there is no
    /// address list, and the salt they were made with is gone.
    pub fn saw(&self, addr: IpAddr, now: &str) {
        let mut state = self.state.lock().expect("the counters are not poisoned");
        state.roll(&day_of(now));
        let mut hasher = Sha256::new();
        hasher.update(state.salt);
        match addr {
            IpAddr::V4(v4) => hasher.update(v4.octets()),
            IpAddr::V6(v6) => hasher.update(v6.octets()),
        }
        let digest = hasher.finalize();
        let mut short = [0u8; 8];
        short.copy_from_slice(&digest[..8]);
        state.seen.insert(short);
    }

    /// Adds what has accumulated to the table and clears the in-memory tally.
    ///
    /// Additive rather than replacing, so two processes counting into the same database sum
    /// instead of overwriting each other, and so a flush is safe to run as often as it likes.
    /// `unique_visitors` is the exception and is written as a **maximum**: the set only grows
    /// through the day, so adding it up would count every visitor once per flush.
    pub fn flush(&self, db: &Db, now: &str) -> Result<(), DbError> {
        let (day, counts, uniques) = {
            let mut state = self.state.lock().expect("the counters are not poisoned");
            state.roll(&day_of(now));
            let counts: Vec<(&'static str, u64)> = state.counts.drain().collect();
            (state.day.clone(), counts, state.seen.len() as u64)
        };

        let locale = self.locale.code();
        db.write(|tx| {
            for (metric, count) in &counts {
                tx.execute(
                    "INSERT INTO daily_metric (day, locale, metric, count) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (day, locale, metric)
                     DO UPDATE SET count = count + excluded.count",
                    params![day, locale, metric, count],
                )?;
            }
            if uniques > 0 {
                tx.execute(
                    "INSERT INTO daily_metric (day, locale, metric, count) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (day, locale, metric)
                     DO UPDATE SET count = MAX(count, excluded.count)",
                    params![day, locale, name::UNIQUE_VISITORS, uniques],
                )?;
            }
            Ok(())
        })
    }
}

impl Counters {
    /// Midnight. The day's counts have already been flushed by the time this matters — the flush
    /// rolls first — and the salt and the set go, which is the whole of T113's promise.
    fn roll(&mut self, today: &str) {
        if self.day == today {
            return;
        }
        self.day = today.to_string();
        self.counts.clear();
        self.salt = rand::rng().random();
        self.seen.clear();
    }
}

/// The date out of an RFC 3339 instant. Both are UTC, so this is the first ten characters.
fn day_of(now: &str) -> String {
    now.get(..10).unwrap_or(now).to_string()
}

/// One day's figures, as `metrics --since` prints them.
#[derive(Debug, PartialEq, Eq)]
pub struct Day {
    pub day: String,
    pub locale: String,
    pub metric: String,
    pub count: i64,
}

/// Everything counted on or after `since`, oldest first.
pub fn since(db: &Db, since: &str) -> Result<Vec<Day>, DbError> {
    let reader = db.reader()?;
    let mut stmt = reader.prepare(
        "SELECT day, locale, metric, count FROM daily_metric
          WHERE day >= ?1
          ORDER BY day, locale, metric",
    )?;
    let rows = stmt
        .query_map(params![since], |r| {
            Ok(Day {
                day: r.get(0)?,
                locale: r.get(1)?,
                metric: r.get(2)?,
                count: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn counted(db: &Db, day: &str, metric: &str) -> i64 {
        db.reader()
            .unwrap()
            .query_row(
                "SELECT count FROM daily_metric WHERE day = ?1 AND metric = ?2",
                params![day, metric],
                |r| r.get(0),
            )
            .unwrap_or(0)
    }

    #[test]
    fn counts_accumulate_and_a_second_flush_adds_to_the_first() {
        let db = Db::open_in_memory().unwrap();
        let metrics = Metrics::new(Locale::De, "2026-07-28T09:00:00Z");

        for _ in 0..3 {
            metrics.count(name::PAGE_VIEW, "2026-07-28T09:00:00Z");
        }
        metrics.flush(&db, "2026-07-28T09:00:00Z").unwrap();
        assert_eq!(counted(&db, "2026-07-28", name::PAGE_VIEW), 3);

        metrics.count(name::PAGE_VIEW, "2026-07-28T09:30:00Z");
        metrics.flush(&db, "2026-07-28T09:30:00Z").unwrap();
        assert_eq!(
            counted(&db, "2026-07-28", name::PAGE_VIEW),
            4,
            "a flush adds, so two processes can count into one database"
        );
    }

    /// The set only grows through the day, so a summed unique count would count every visitor once
    /// per flush. Written as a maximum instead.
    #[test]
    fn unique_visitors_are_a_maximum_and_not_a_sum() {
        let db = Db::open_in_memory().unwrap();
        let metrics = Metrics::new(Locale::De, "2026-07-28T09:00:00Z");
        let now = "2026-07-28T09:00:00Z";

        for last in 1..=3u8 {
            metrics.saw(IpAddr::V4(Ipv4Addr::new(203, 0, 113, last)), now);
        }
        // The same visitor again is not a second visitor.
        metrics.saw(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)), now);

        metrics.flush(&db, now).unwrap();
        metrics.flush(&db, now).unwrap();
        assert_eq!(counted(&db, "2026-07-28", name::UNIQUE_VISITORS), 3);
    }

    /// T113: the salt and the set are discarded at rollover. Observable from outside as the count
    /// starting again — the same address on the new day is a new visitor, because it has to be:
    /// nothing that could recognise it survived midnight.
    #[test]
    fn the_visitor_set_and_its_salt_do_not_survive_the_day() {
        let db = Db::open_in_memory().unwrap();
        let metrics = Metrics::new(Locale::De, "2026-07-28T23:59:00Z");
        let visitor = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));

        metrics.saw(visitor, "2026-07-28T23:59:00Z");
        metrics.count(name::PAGE_VIEW, "2026-07-28T23:59:00Z");
        metrics.flush(&db, "2026-07-28T23:59:00Z").unwrap();

        metrics.saw(visitor, "2026-07-29T00:01:00Z");
        metrics.flush(&db, "2026-07-29T00:01:00Z").unwrap();

        assert_eq!(counted(&db, "2026-07-28", name::UNIQUE_VISITORS), 1);
        assert_eq!(counted(&db, "2026-07-29", name::UNIQUE_VISITORS), 1);
        assert_eq!(
            counted(&db, "2026-07-29", name::PAGE_VIEW),
            0,
            "yesterday's counts do not leak into today"
        );
    }

    #[test]
    fn the_reader_returns_what_was_counted_from_a_date_onwards() {
        let db = Db::open_in_memory().unwrap();
        let metrics = Metrics::new(Locale::De, "2026-07-27T09:00:00Z");
        metrics.count(name::PAGE_VIEW, "2026-07-27T09:00:00Z");
        metrics.flush(&db, "2026-07-27T09:00:00Z").unwrap();
        metrics.count(name::ACCOUNT_CREATED, "2026-07-28T09:00:00Z");
        metrics.flush(&db, "2026-07-28T09:00:00Z").unwrap();

        let all = since(&db, "2026-07-27").unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].day, "2026-07-27");

        let recent = since(&db, "2026-07-28").unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].metric, name::ACCOUNT_CREATED);
        assert_eq!(recent[0].locale, "de");
        assert_eq!(recent[0].count, 1);
    }
}
