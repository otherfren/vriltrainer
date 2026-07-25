//! Block-wise advancement (FR-019, D17).
//!
//! The reported figures advance per completed block of trials rather than after every single one.
//! This blunts optional stopping: a user who plays until the number looks good and then stops
//! otherwise inflates the false-positive rate far past 5 %, which would make the site's own
//! headline figure dishonest by construction.
//!
//! What it applies to is a distinction worth keeping straight. Optional stopping biases an
//! *inference*, not a *record*: the deviation, the by-chance figure and the Wilson bound are held
//! still between boundaries, while completed trials, hits and the abandoned count are reported live
//! because stopping cannot flatter them.

/// How many trials are inside reported statistics, given `completed` and the configured block
/// size. Always a multiple of the block size, so the number on screen never moves mid-block.
///
/// A block size of zero would mean "no blocks", which is a configuration nobody should be able to
/// reach; it is read as one trial per block rather than as a division by zero.
pub fn reported_trials(completed: u64, block_size: u64) -> u64 {
    if block_size == 0 {
        return completed;
    }
    completed - completed % block_size
}

/// The `n` the displayed figures actually stand over, once the statistics view has unlocked.
///
/// The unlock is itself the first boundary, and it has to be: with the shipped numbers the view
/// appears at ten completed trials while a block is twenty-five, so a bare block floor would unlock
/// a page whose every figure was computed over zero trials. Below the unlock this is zero, because
/// there is no reported figure at all yet.
pub fn reported(completed: u64, block_size: u64, unlock_at: u64) -> u64 {
    if completed < unlock_at {
        return 0;
    }
    reported_trials(completed, block_size).max(unlock_at)
}

/// Whether `completed` is the exact count at which the reported figures may move.
///
/// Paired with [`reported`], which must always name the last boundary at or below `completed` —
/// otherwise a stored figure and the `n` printed beside it describe different sets of trials, and
/// the page invites a reader to divide one by the other and get nonsense.
pub fn is_boundary(completed: u64, block_size: u64, unlock_at: u64) -> bool {
    if completed < unlock_at {
        return false;
    }
    completed == unlock_at || (block_size != 0 && completed.is_multiple_of(block_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: u64 = 25;
    const UNLOCK: u64 = 10;

    #[test]
    fn the_figure_stands_still_inside_a_block() {
        assert_eq!(reported_trials(24, BLOCK), 0);
        assert_eq!(reported_trials(25, BLOCK), 25);
        assert_eq!(reported_trials(49, BLOCK), 25);
        assert_eq!(reported_trials(50, BLOCK), 50);
    }

    /// FR-017 unlocks the page at ten while D17 sets the block at twenty-five. Without the unlock
    /// counting as a boundary the page would appear showing statistics over nothing.
    #[test]
    fn the_unlock_is_the_first_boundary() {
        assert_eq!(reported(9, BLOCK, UNLOCK), 0);
        assert_eq!(reported(10, BLOCK, UNLOCK), 10);
        assert_eq!(reported(24, BLOCK, UNLOCK), 10);
        assert_eq!(reported(25, BLOCK, UNLOCK), 25);
    }

    /// The invariant the storage relies on: what a resolve stores at a boundary is what a later
    /// read prints an `n` for. Swept over an unlock below the block size and one above it, since
    /// D26 expects both numbers to be moved by an operator.
    #[test]
    fn every_reported_n_is_the_last_boundary_below_it() {
        for (block, unlock) in [(25u64, 10u64), (25, 60), (5, 5), (1, 10)] {
            let mut last = 0;
            for completed in 0..=300u64 {
                if is_boundary(completed, block, unlock) {
                    last = completed;
                }
                assert_eq!(
                    reported(completed, block, unlock),
                    last,
                    "block {block}, unlock {unlock}, completed {completed}"
                );
            }
        }
    }

    /// The reported count never goes backwards as trials accumulate — a figure that fell would
    /// read as the site taking a result away.
    #[test]
    fn the_reported_count_only_grows() {
        let mut previous = 0;
        for completed in 0..=500u64 {
            let n = reported(completed, BLOCK, UNLOCK);
            assert!(n >= previous);
            assert!(n <= completed);
            previous = n;
        }
    }
}
