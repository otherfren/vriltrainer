//! `account_stats`, maintained on resolve rather than computed per request.

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

use rusqlite::Transaction;

use crate::db::DbError;

/// Folds one resolved trial into the account's totals.
///
/// Designed to be passed to [`crate::db::Db::append_with`], so the figures and the log entry they
/// describe commit in the same transaction. Anything else allows a state where the record says one
/// thing and the leaderboard says another, and the record is the one that is published.
pub fn on_resolve(
    tx: &Transaction<'_>,
    account_id: &str,
    hit: bool,
    at: &str,
) -> Result<(), DbError> {
    todo!("T038")
}

/// Counts a trial the account never completed. Abandonment is the absence of a resolve entry, so
/// this is a convenience for the statistics page and never a source of truth — the export is
/// (FR-027, SC-012).
pub fn recount_abandoned(tx: &Transaction<'_>, account_id: &str) -> Result<u64, DbError> {
    todo!("T038")
}
