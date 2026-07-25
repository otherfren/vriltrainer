//! Leaderboard eligibility (FR-040, D17, D21).

use crate::config::Thresholds;

/// Completed trials across at least the configured number of distinct UTC days.
///
/// The day count is the only cheap measure that resists parallel farming, because parallelism does
/// not compress the calendar: a farm has to keep its accounts alive and playing across three days
/// instead of running them through over lunch, which moves the cost from "script" to
/// "infrastructure". For a genuine user it describes what they already do.
pub fn is_eligible(completed: u64, distinct_utc_days: u32, t: &Thresholds) -> bool {
    completed >= t.eligibility_trials as u64 && distinct_utc_days >= t.eligibility_days
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The enthusiast who plays 150 trials in one evening is not ranked the next morning. That is
    /// the cost D21 accepted, and it is answered in the interface with "2 more days until ranked"
    /// rather than with silence.
    #[test]
    fn trials_alone_are_not_enough() {
        let t = Thresholds::default();
        assert!(!is_eligible(150, 1, &t));
        assert!(!is_eligible(99, 30, &t));
        assert!(is_eligible(100, 3, &t));
    }
}
