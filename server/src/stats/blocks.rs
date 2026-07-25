//! Block-wise advancement (FR-019, D17).
//!
//! The reported figures advance per completed block of trials rather than after every single one.
//! This blunts optional stopping: a user who plays until the number looks good and then stops
//! otherwise inflates the false-positive rate far past 5 %, which would make the site's own
//! headline figure dishonest by construction.

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

/// How many trials are inside reported statistics, given `completed` and the configured block
/// size. Always a multiple of the block size, so the number on screen never moves mid-block.
pub fn reported_trials(completed: u64, block_size: u64) -> u64 {
    todo!("T041")
}
