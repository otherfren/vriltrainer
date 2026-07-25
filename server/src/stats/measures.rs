//! The two headline figures, and the reason they are different figures.
//!
//! `deviation` measures evidence against chance; `wilson_lower` estimates ability. A lucky run of
//! 100 trials at 25 % gives z = 3.78 and would outrank a steadier 1000 trials at 15 % (z = 2.39),
//! which is why D20 sorts the board on the Wilson bound: trial count has to matter.

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

/// The chance rate, `1/8` (D8).
pub const CHANCE: f64 = 0.125;

/// The confidence level the board's sort key is computed at.
///
/// Not a defence against multi-accounting and not chosen as one — D27 showed the penalty at fixed
/// `n` is fixed while a farmer's max-of-K gain grows like `sqrt(2 ln K)`, so raising this flips
/// the comparison at ten accounts and loses again at a hundred. It is here because it is the right
/// thing to *show*.
pub const WILSON_Z: f64 = 1.96;

/// The lower bound of the Wilson score interval for `hits` out of `n`.
///
/// Small samples are penalised automatically, so four trials and four hits does not top the table
/// and no arbitrary minimum-trials rule is needed for that (D8).
pub fn wilson_lower(hits: u64, n: u64, z: f64) -> f64 {
    todo!("T039")
}

/// Standard deviations from chance. The sign matters: the ladder is symmetric and the low tail is
/// as much of a finding as the high one.
pub fn deviation(hits: u64, n: u64) -> f64 {
    todo!("T039")
}
