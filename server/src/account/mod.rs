//! Accounts and the capability token (D9).
//!
//! There is no registration, no email and no password. The user invents a name, the server issues
//! a secret access link, and that link is the account. Losing it loses the history, deliberately:
//! a recovery path is an authentication surface to maintain for a case the user was warned about
//! twice, and email recovery would pull real GDPR obligations onto a solo operator.

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

pub mod handoff;
pub mod name;
pub mod name_filter;

use crate::db::{Db, DbError};

/// A freshly created account. The `access_token` field is the only place the token exists in full
/// — the database holds a hash — so this value is returned to the caller once and then dropped.
pub struct NewAccount {
    /// The opaque internal identifier. This, and never the name, is what appears in the log.
    pub id: String,
    /// Shown beside the name on public surfaces (FR-029). Drawn independently of the token:
    /// deriving it would publish a function of the secret.
    pub public_id: String,
    pub access_token: String,
}

/// At least 128 bits from a CSPRNG (D9). Returned once and never again.
pub fn mint_token() -> String {
    todo!("T026")
}

/// The token is a password and is treated as one: only this ever reaches the database.
pub fn token_hash(token: &str) -> String {
    todo!("T026")
}

/// Creates an account with `name` in the `pending` state of D25 — nothing the user types is
/// public until a human has approved it.
pub fn create(db: &Db, name: &str, now: &str) -> Result<NewAccount, DbError> {
    todo!("T026")
}

/// The account id a bearer token belongs to, if any.
pub fn authenticate(db: &Db, token: &str) -> Result<Option<String>, DbError> {
    todo!("T026")
}

/// Removes the display name, satisfying erasure (FR-035). The account's trials stay in the log
/// under its opaque identifier and every proof over them still verifies (FR-036) — which is the
/// entire reason names were kept out of the chain in the first place.
pub fn forget_name(db: &Db, account_id: &str, now: &str) -> Result<(), DbError> {
    todo!("T069")
}
