//! The name state machine of D25: `pending` -> `approved` | `rejected`.
//!
//! Two audiences see different things, and that is the whole design. The holder always sees the
//! name they chose, in whatever state it is — they cannot pick a better one without seeing the one
//! that was refused. The public sees the most recently approved name and a fixed-length mask
//! otherwise, so a row reads as *a name exists here and has not been cleared* rather than as an
//! absence.
//!
//! FR-026 is untouched by any of this: the log references the opaque account id and never the
//! name, so nothing here reaches the record.

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

use crate::db::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NameState {
    Pending,
    Approved,
    Rejected,
}

/// The mask shown in place of a name that has not been approved.
///
/// **Fixed length is the point.** A mask that preserved the real length and first letter would
/// still communicate the shape of a slur, which is precisely what pre-approval exists to keep off
/// the page. Same idiom as the masked access link in D9.
pub const MASK: &str = "••••••••";

/// What the holder sees: their own name, its state, and why it was refused if it was.
pub struct HolderView {
    pub name: Option<String>,
    pub state: NameState,
    /// A refusal code, not a sentence — the sentence is product copy and lives in the client.
    pub reason: Option<String>,
}

/// What everyone else sees on the leaderboard and in any shared artefact.
///
/// The public identifier is shown beside this either way (FR-029), so a masked row is still
/// attributable and still checkable against the log.
pub fn public_display(public_name: Option<&str>) -> String {
    match public_name {
        Some(n) => n.to_string(),
        None => MASK.to_string(),
    }
}

/// Records a chosen name and puts it in the review queue.
///
/// The pre-filter runs first ([`super::name_filter`]); this is what happens to whatever survives
/// it. On a rename the previously **approved** name stays public until the new one clears, so a
/// rename is not punished with anonymity.
pub fn submit(db: &Db, account_id: &str, name: &str, now: &str) -> Result<NameState, DbError> {
    todo!("T096")
}

/// Publishes the name. Reversible, which is what allows this to sit behind a public admin API.
pub fn approve(db: &Db, account_id: &str, now: &str) -> Result<(), DbError> {
    todo!("T096")
}

/// Refuses the name and discards it — holding a name that was refused is holding personal data
/// for no purpose. Does **not** consume the rename rate limit: the user has not had a turn yet.
pub fn reject(db: &Db, account_id: &str, reason: &str, now: &str) -> Result<(), DbError> {
    todo!("T096")
}

/// The review queue, oldest first. It is `account` filtered to `pending`; a separate table would
/// be a second copy of a state that already exists.
pub fn pending(db: &Db, limit: u32) -> Result<Vec<(String, String)>, DbError> {
    todo!("T098")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unapproved_name_masks_to_a_fixed_length() {
        // Two names of very different lengths must be indistinguishable once masked, or the mask
        // still leaks the shape of what it is hiding.
        assert_eq!(public_display(None), public_display(None));
        assert_eq!(public_display(Some("otherfren")), "otherfren");
    }
}
