//! Single-use handoff codes for the language switch (D11, FR-031).
//!
//! `vriltrainer.de` and `vriltrainer.com` are separate origins, so local storage does not travel
//! with the user and a naive switch arrives as an anonymous first-time visitor — losing the
//! progress and creating a duplicate account, which would put one person into the leaderboard and
//! the aggregate twice.
//!
//! The obvious fix, putting the long-lived token into the target URL, is the one thing this must
//! not do: it would place the secret in the address bar and in the target domain's history, and
//! streaming is a stated use case. Hence a code that is worth thirty seconds and one redemption.

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

use crate::db::{Db, DbError};

/// How long a code is worth anything.
pub const LIFETIME_SECONDS: i64 = 30;

/// Mints a code for `account_id`. Only its hash is stored, the same discipline as the access
/// token: a code is a bearer credential for the whole account, briefly.
pub fn mint(db: &Db, account_id: &str, now: &str) -> Result<String, DbError> {
    todo!("T065")
}

/// Burns the code and returns a fresh access token for the account it belonged to.
///
/// Single use is enforced by the burn happening in the same transaction as the lookup, or two
/// concurrent redemptions both succeed.
pub fn redeem(db: &Db, code: &str, now: &str) -> Result<Option<String>, DbError> {
    todo!("T065")
}
