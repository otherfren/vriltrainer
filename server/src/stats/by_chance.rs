//! "How many out of ten thousand would reach this by luck alone."
//!
//! Without this line the z-score is not interpretable, and a number nobody can interpret on a page
//! about psi is a number that will be interpreted generously (D8, R3).

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

/// The exact binomial tail, phrased per ten thousand.
///
/// Exact rather than normal-approximated: small `n` is where the approximation is worst and also
/// where the claim is loudest, because the statistics page unlocks at ten trials.
pub fn per_10_000(hits: u64, n: u64) -> u32 {
    todo!("T040")
}
