//! When an answer may be evaluated at all (FR-038, FR-039).
//!
//! This is one function because the order of the two questions is the whole point, and an order
//! is not something to leave to each call site. The minimum viewing time must be decided
//! **before** the chosen image is looked at (SC-016): checked afterwards, the refusal itself
//! answers "was that the target?" for anyone willing to submit a guess in under three seconds and
//! read the status code. That is why [`gate`] is handed the clock and nothing else — it cannot
//! consult the answer even by accident, and a handler that calls it first has no way to have
//! examined the choice yet.
//!
//! Neither outcome writes anything. A speed-rejected answer does not consume the trial (FR-037):
//! the user is told to look for longer and answers again.

use crate::trial::token::TokenTwo;

/// Whether this answer may be scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    /// Score it.
    Evaluate,
    /// Under the minimum viewing time. The trial stays open (425).
    TooFast,
    /// The validity period has elapsed; the trial can never resolve now (410).
    Expired,
}

/// `now` is Unix seconds.
///
/// Expiry is asked first. Both can hold only if the lifetime is shorter than the minimum viewing
/// time, which is a misconfiguration, and "this trial is over" is the more useful of the two
/// answers: it is the one the interface must turn into a fresh trial rather than an invitation to
/// look for longer (FR-038 — never silently scored as a miss).
pub fn gate(token: &TokenTwo, now: i64, min_view_seconds: i64) -> Timing {
    if !token.is_live(now) {
        return Timing::Expired;
    }
    if !token.viewed_long_enough(now, min_view_seconds) {
        return Timing::TooFast;
    }
    Timing::Evaluate
}

/// Now, in the unit the sealed token measures in.
pub fn now_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REVEALED: i64 = 1_800_000_000;
    const EXPIRES: i64 = REVEALED + 24 * 3600;

    fn token() -> TokenTwo {
        TokenTwo {
            s_server: vec![1; 32],
            s_client: vec![2; 32],
            nonce: vec![3; 32],
            coordinate: "4821-9037".into(),
            pool_version: 1,
            selected: vec![0, 1, 2, 3, 4, 5, 6, 7],
            target_slot: 2,
            display_order: vec![7, 6, 5, 4, 3, 2, 1, 0],
            revealed_at: REVEALED,
            expires_at: EXPIRES,
        }
    }

    #[test]
    fn an_answer_inside_the_minimum_viewing_time_is_refused() {
        assert_eq!(gate(&token(), REVEALED, 3), Timing::TooFast);
        assert_eq!(gate(&token(), REVEALED + 2, 3), Timing::TooFast);
    }

    /// The boundary, spelled out: three seconds means three, not four.
    #[test]
    fn the_minimum_is_inclusive() {
        assert_eq!(gate(&token(), REVEALED + 3, 3), Timing::Evaluate);
    }

    #[test]
    fn an_answer_after_the_validity_period_is_gone() {
        assert_eq!(gate(&token(), EXPIRES, 3), Timing::Expired);
        assert_eq!(gate(&token(), EXPIRES + 1, 3), Timing::Expired);
        assert_eq!(gate(&token(), EXPIRES - 1, 3), Timing::Evaluate);
    }

    /// A clock that has gone backwards — an NTP step, a restored snapshot — must not make a trial
    /// answerable in nothing flat. `now - revealed_at` is negative, which is not "long enough".
    #[test]
    fn a_clock_that_moved_backwards_still_refuses() {
        assert_eq!(gate(&token(), REVEALED - 60, 3), Timing::TooFast);
    }

    /// D26 keeps the minimum in configuration, so zero is expressible. It must mean "no wait",
    /// not "never".
    #[test]
    fn a_minimum_of_zero_evaluates_immediately() {
        assert_eq!(gate(&token(), REVEALED, 0), Timing::Evaluate);
    }
}
