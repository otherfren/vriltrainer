//! The pre-filter, ported from the client's `checkDisplayName` (D25, T097).
//!
//! Since D25 this is no longer the gate — a human approves every name that reaches the board — it
//! is what keeps the review queue short. The client runs the same rules so it can tell somebody
//! what is wrong while they type, but that copy is UX only: a rule checked only in the client is
//! not checked.
//!
//! Both implementations must agree, so `client/src/app/core/display-name.ts` and this file move
//! together or names start being accepted in one place and refused in the other.

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

pub const NAME_MIN: usize = 3;
pub const NAME_MAX: usize = 20;

/// Why a name was refused.
///
/// A code rather than a sentence. The sentence is product copy, it differs per domain (D10), and
/// it belongs in the client's message catalogue — putting German in a Rust file would be the one
/// thing CLAUDE.md forbids outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    TooShort,
    TooLong,
    /// No letters at all: `1488`, punctuation, whitespace.
    Shapeless,
    /// A name the interface uses for itself.
    Reserved,
    Hate,
    Vulgar,
    /// An address of some kind — a URL, an email, a handle pretending to be one.
    Address,
}

/// Folds leet substitutions before the word lists are applied, so `h1tl3r` and `f0tze` are caught
/// by the same entry as the plain spelling. Only substitutions that read as the letter: anything
/// more aggressive starts matching ordinary names.
pub fn fold(name: &str) -> String {
    todo!("T097")
}

pub fn check(name: &str) -> Result<(), Refusal> {
    todo!("T097")
}
