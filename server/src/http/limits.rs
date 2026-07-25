//! The two abuse limits D17 settled on: accounts per client address, open trials per account.
//!
//! They sit together because they answer the same problem from two ends. Every trial is a
//! permanent entry in a public log, and an account is free to create. Without the first, one
//! machine mints accounts without limit; without the second, one account holds unlimited
//! uncompleted trials and each is a row nobody can ever delete. Neither is a rate limit over
//! time — D17 chose a cap on *concurrent* trials precisely because it bounds the log's growth
//! per account per trial lifetime rather than merely slowing it down.
//!
//! The per-address counter lives in memory and nowhere else. D28 forbids a per-visitor row, and a
//! table of addresses and creation times is exactly that: something to leak, to be subpoenaed for,
//! and to regret. The cost is stated below.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use rusqlite::{Transaction, params};

use crate::db::DbError;

/// The window the per-address limit counts over. `accounts_per_address_per_hour` names it.
const WINDOW_SECONDS: i64 = 3600;

/// Addresses remembered at once. Each costs a key and a handful of timestamps, so this is a few
/// megabytes at the ceiling — and the ceiling is only reached by a flood from tens of thousands of
/// distinct addresses within one hour.
const MAX_TRACKED: usize = 100_000;

/// Creations per address inside the window.
///
/// Two properties are deliberate. It is **per process**, so with D24's two processes an address
/// gets the configured allowance on each domain; the alternative is shared state in the database,
/// which is the per-visitor row D28 refuses, and doubling a small allowance is the cheaper harm.
/// And it **fails open** when it runs out of room: an address it has forgotten is admitted. This
/// limit exists to make casual account farming tedious, not to stop a botnet — D9 says plainly
/// that Sybil accounts are answered on the display side, not here — and a limiter that fails
/// closed under load stops account creation for everyone, which is a worse day than the one it
/// prevents.
pub struct CreationLimit {
    seen: Mutex<HashMap<IpAddr, Vec<i64>>>,
}

impl CreationLimit {
    pub fn new() -> Self {
        CreationLimit {
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Whether another account may be created from `addr` now.
    ///
    /// Separate from [`CreationLimit::record`] because a refused name must not spend the
    /// allowance: the pre-filter turns typos away (D25), and a user who is told "that name is too
    /// short" five times has still never created an account.
    pub fn admits(&self, addr: IpAddr, now: i64, per_window: u32) -> bool {
        let seen = lock(&self.seen);
        let recent = seen.get(&addr).map_or(0, |at| {
            at.iter().filter(|t| **t > now - WINDOW_SECONDS).count()
        });
        recent < per_window as usize
    }

    /// Counts one creation against `addr`.
    pub fn record(&self, addr: IpAddr, now: i64) {
        let mut seen = lock(&self.seen);
        if seen.len() >= MAX_TRACKED {
            seen.retain(|_, at| at.iter().any(|t| *t > now - WINDOW_SECONDS));
        }
        if seen.len() >= MAX_TRACKED && !seen.contains_key(&addr) {
            // Nothing left to forget. See the type's note on failing open.
            warn_full();
            return;
        }
        let at = seen.entry(addr).or_default();
        at.retain(|t| *t > now - WINDOW_SECONDS);
        at.push(now);
    }
}

impl Default for CreationLimit {
    fn default() -> Self {
        Self::new()
    }
}

/// The process's limiter.
///
/// A global rather than a field on the application state: the state is rebuilt per test and the
/// counter is deliberately not per-request state, so there is exactly one of these and its
/// lifetime is the process's. Tests exercise [`CreationLimit`] directly.
pub fn creation() -> &'static CreationLimit {
    static CREATION: LazyLock<CreationLimit> = LazyLock::new(CreationLimit::new);
    &CREATION
}

fn warn_full() {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            tracked = MAX_TRACKED,
            "the per-address creation limit is out of room and is admitting unknown addresses"
        );
    }
}

/// Uncompleted trials the account holds that could still be completed, counted **inside** the
/// caller's write transaction.
///
/// "Could still be completed" is the whole subtlety. A commit with no resolve is an abandoned
/// trial (D2) and abandonment is permanent, so counting every one of them would cap an account's
/// lifetime output rather than its concurrency: a user who walks away from three trials could
/// never start another. The lifetime of D16 is what separates the two, and it runs from the commit
/// — the same clock the answer path expires a trial by, so a trial stops counting against this cap
/// at the exact moment it becomes unanswerable. Two rules that disagreed here would leave a trial
/// that blocks the cap and cannot be finished, which is a dead account.
///
/// `exclude` is the trial being appended: `also` runs after the insert, so the new commit is
/// already visible to this query.
pub fn open_trials(
    tx: &Transaction<'_>,
    account_id: &str,
    not_before: &str,
    exclude: &str,
) -> Result<u32, DbError> {
    let open = tx.query_row(
        "SELECT COUNT(*) FROM log_entry c
          WHERE c.kind = 'commit' AND c.account_id = ?1 AND c.at >= ?2 AND c.trial_id <> ?3
            AND NOT EXISTS (SELECT 1 FROM log_entry r
                             WHERE r.trial_id = c.trial_id AND r.kind = 'resolve')",
        params![account_id, not_before, exclude],
        |r| r.get(0),
    )?;
    Ok(open)
}

/// A poisoned lock means a previous caller panicked while counting. Refusing every account
/// creation from then on would turn one panic into an outage; see `db::lock`.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn an_address_may_create_up_to_the_limit_and_no_more() {
        let limit = CreationLimit::new();
        let addr = ip("203.0.113.7");
        for _ in 0..5 {
            assert!(limit.admits(addr, NOW, 5));
            limit.record(addr, NOW);
        }
        assert!(!limit.admits(addr, NOW, 5));
    }

    #[test]
    fn addresses_are_counted_apart() {
        let limit = CreationLimit::new();
        for _ in 0..5 {
            limit.record(ip("203.0.113.7"), NOW);
        }
        assert!(!limit.admits(ip("203.0.113.7"), NOW, 5));
        assert!(limit.admits(ip("198.51.100.9"), NOW, 5));
    }

    #[test]
    fn the_window_slides() {
        let limit = CreationLimit::new();
        let addr = ip("203.0.113.7");
        for _ in 0..5 {
            limit.record(addr, NOW);
        }
        assert!(!limit.admits(addr, NOW + WINDOW_SECONDS - 1, 5));
        assert!(limit.admits(addr, NOW + WINDOW_SECONDS, 5));
    }

    /// The reason the check and the count are two calls: the pre-filter refuses names, and a
    /// refusal has created nothing.
    #[test]
    fn asking_is_not_creating() {
        let limit = CreationLimit::new();
        let addr = ip("203.0.113.7");
        for _ in 0..20 {
            assert!(limit.admits(addr, NOW, 5));
        }
    }
}
