//! Share-based ranks (D23, FR-042).
//!
//! Bands are shares, not seats. D19 was right that supply must be fixed rather than earned by
//! hitting a threshold — absolute thresholds would have minted twenty-eight archons on a
//! thousand-visitor launch day — and wrong to fix it as a seat count: third place out of ten is
//! nothing, third place out of two hundred thousand is a title. A share means the same thing at
//! every population, which is the only form that survives the site growing.

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

use crate::config::{RankBand, Thresholds};

/// The band a place on the board falls into, or `None` for the middle 60 % — Normie, and the
/// honest answer for almost everyone.
///
/// A band exists only once `share * eligible >= 1`, with **no rounding up**: rounding would hand
/// out the rarest title at any population, which is the opposite of what a share means. The top
/// rung is therefore unreachable until a thousand people have taken this seriously.
pub fn band_for(place: u64, eligible: u64, t: &Thresholds) -> Option<&RankBand> {
    todo!("T056")
}

/// Recomputes and materialises every rank, returning the number of eligible accounts.
///
/// Runs every ~15 minutes rather than per request or per block, so the board is a read of one
/// table and stays stable between passes. The board states when this last ran — a rank that has
/// not moved otherwise reads as a bug.
pub fn recompute(db: &crate::db::Db, t: &Thresholds, now: &str) -> Result<u64, crate::db::DbError> {
    todo!("T056")
}
