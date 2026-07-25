//! "How many out of ten thousand would reach this by luck alone."
//!
//! Without this line the z-score is not interpretable, and a number nobody can interpret on a page
//! about psi is a number that will be interpreted generously (D8, R3).

use super::measures::CHANCE;

/// The exact binomial tail, phrased per ten thousand.
///
/// Exact rather than normal-approximated: small `n` is where the approximation is worst and also
/// where the claim is loudest, because the statistics page unlocks at ten trials.
///
/// R3 names the upper tail `P(X >= hits)`. Below the expected count that would answer the wrong
/// question — a Kartoffel with no hits in 120 trials would be told that 10,000 in 10,000 manage it
/// by luck, when the truth is that almost nobody does. So the tail is taken on the side the result
/// actually fell, which is also what R5 requires of the low end: the same statistical distance,
/// applied downward, or the two tail counts stop being comparable and the site loses its most
/// legible honest signal.
pub fn per_10_000(hits: u64, n: u64) -> u32 {
    if n == 0 {
        // No trials, no surprise. Everybody reaches nothing by luck.
        return 10_000;
    }
    let hits = hits.min(n);
    let tail = if hits as f64 >= n as f64 * CHANCE {
        binomial_tail(hits, n, n)
    } else {
        binomial_tail(0, hits, n)
    };
    // Rounded, so a result rarer than one in twenty thousand reports zero. The client says "fewer
    // than 1 in 10,000" for that; rounding it up to 1 instead would understate the rarest results
    // the site can produce, which are the ones a reader most needs the number for.
    (tail * 10_000.0).round().clamp(0.0, 10_000.0) as u32
}

/// `P(from <= X <= to)` for `X ~ Binomial(n, CHANCE)`.
///
/// The probabilities are summed through their logarithms. The direct form overflows in the
/// binomial coefficient and underflows in `q^n` long before an account's trial count becomes
/// unusual — `0.875^100000` is zero in `f64`, and a tail computed from zeroes is a claim of
/// impossibility the arithmetic did not earn.
fn binomial_tail(from: u64, to: u64, n: u64) -> f64 {
    let ln_p = CHANCE.ln();
    let ln_q = (1.0 - CHANCE).ln();

    // ln C(n, k), carried forward one step at a time: C(n, k) = C(n, k-1) * (n - k + 1) / k.
    let mut ln_choose = 0.0f64;
    let mut sum = 0.0f64;
    for k in 0..=to {
        if k > 0 {
            ln_choose += ((n - k + 1) as f64).ln() - (k as f64).ln();
        }
        if k >= from {
            sum += (ln_choose + k as f64 * ln_p + (n - k) as f64 * ln_q).exp();
        }
    }
    // Floating-point summation of a full distribution can land a hair above one, and a tail of
    // 1.0000000002 becomes 10,001 per 10,000 on the page.
    sum.min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cases small enough to check by hand, which is the only kind of test that can tell an exact
    /// tail from a good approximation.
    #[test]
    fn the_small_cases_are_exactly_right() {
        // P(X >= 1 | n = 1) = 1/8.
        assert_eq!(per_10_000(1, 1), 1250);
        // P(X >= 2 | n = 2) = 1/64.
        assert_eq!(per_10_000(2, 2), 156);
        // Below chance, so the low tail: P(X = 0 | n = 3) = (7/8)^3 = 0.66992…
        assert_eq!(per_10_000(0, 3), 6699);
    }

    /// The normal approximation is worst exactly where the statistics page unlocks, which is why
    /// R3 refuses it. Three hits in ten is 1195 in 10,000 exactly; the normal approximation puts it
    /// near 800, and the difference is the difference between "unusual" and "remarkable".
    #[test]
    fn ten_trials_are_computed_exactly_and_not_approximated() {
        assert_eq!(per_10_000(3, 10), 1195);
    }

    /// The published example's shape, at a size where the sum is long enough for the logarithms to
    /// matter: 21 hits in 120 trials.
    #[test]
    fn a_long_run_stays_accurate() {
        assert_eq!(per_10_000(21, 120), 692);
    }

    /// Rarer results report smaller numbers, on both sides of chance. If this ever inverted, the
    /// page would call the best result on the site the most ordinary one.
    #[test]
    fn the_figure_falls_as_the_result_gets_rarer() {
        let above: Vec<u32> = (15..=30).map(|h| per_10_000(h, 120)).collect();
        assert!(above.windows(2).all(|w| w[0] >= w[1]), "{above:?}");
        let below: Vec<u32> = (0..=15).rev().map(|h| per_10_000(h, 120)).collect();
        assert!(below.windows(2).all(|w| w[0] >= w[1]), "{below:?}");
    }

    /// Equal distances either side of chance produce figures of the same order, which is what R5
    /// requires of the low end: a Kartoffel is told how rare they are in the same terms an Annunaki
    /// is, or the two counts stop being comparable and the site loses its most legible signal. Not
    /// *equal* figures — the binomial is skewed at `p = 1/8` and pretending otherwise would be the
    /// approximation R3 refuses.
    #[test]
    fn the_two_tails_are_reported_in_the_same_terms() {
        let above = per_10_000(120, 800);
        let below = per_10_000(80, 800);
        assert_eq!((above, below), (205, 164));
    }

    /// A large run must not fall out of the arithmetic as a zero, which is what a direct product
    /// of `q^n` would do here.
    #[test]
    fn a_result_at_chance_over_a_hundred_thousand_trials_is_unremarkable() {
        let at_chance = per_10_000(12_500, 100_000);
        assert!(
            (4_500..=5_500).contains(&at_chance),
            "exactly chance reported as {at_chance} in 10,000"
        );
    }

    #[test]
    fn nothing_played_is_nothing_special() {
        assert_eq!(per_10_000(0, 0), 10_000);
    }
}
