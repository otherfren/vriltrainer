//! The two headline figures, and the reason they are different figures.
//!
//! `deviation` measures evidence against chance; `wilson_lower` estimates ability. A lucky run of
//! 100 trials at 25 % gives z = 3.78 and would outrank a steadier 1000 trials at 15 % (z = 2.39),
//! which is why D20 sorts the board on the Wilson bound: trial count has to matter.

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
///
/// Zero when there is nothing to go on. That is not a neutral default dressed up as a number: the
/// bound is a *guaranteed minimum* rate, and the guaranteed minimum from no evidence is nothing. It
/// also sorts an account with no completed trials to the bottom of a board ordered on this column,
/// which a NaN would not.
pub fn wilson_lower(hits: u64, n: u64, z: f64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let n = n as f64;
    let p = hits as f64 / n;
    let z2 = z * z;

    let denominator = 1.0 + z2 / n;
    let centre = p + z2 / (2.0 * n);
    let margin = z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();

    // Clamped rather than left to go negative. A negative lower bound is arithmetically correct
    // and unpublishable — "verified minimum rate: −1.2 %" is not a sentence, and D20 puts this
    // column on the board verbatim.
    ((centre - margin) / denominator).max(0.0)
}

/// The upper bound of the same interval: the highest rate the record is still consistent with.
///
/// This exists because [`wilson_lower`] carries **no information below chance**. For an account
/// with no hits at all the lower bound is not merely clamped to zero, it *is* zero exactly, at
/// every `n` — `centre` and `margin` are both `z²/2n` and cancel. So the entire low tail ties on
/// that column, and a board ordered on it alone hands the low end to whichever tie-break follows.
/// Ordering by `completed` there inverts the tail outright: three hundred trials without a hit is
/// stronger evidence of an anti-talent than a hundred, and it would have placed higher.
///
/// The upper bound is informative exactly where the lower one is not. With no hits it falls as `n`
/// grows — 0/100 admits 3.7 %, 0/300 only 1.3 % — so it orders the low tail the way the lower bound
/// orders the high one. That is what makes D23's ladder actually symmetric rather than symmetric in
/// its titles only.
///
/// One at an empty record, for the same reason its counterpart is zero: with no evidence, nothing
/// is ruled out.
pub fn wilson_upper(hits: u64, n: u64, z: f64) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let n = n as f64;
    let p = hits as f64 / n;
    let z2 = z * z;

    let denominator = 1.0 + z2 / n;
    let centre = p + z2 / (2.0 * n);
    let margin = z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();

    ((centre + margin) / denominator).min(1.0)
}

/// `hits / n`, and zero for an empty record.
///
/// Zero rather than the NaN the division produces: the value is serialised into JSON, and
/// `serde_json` writes a NaN as `null`, which the client would then render as a blank where a
/// percentage belongs.
pub fn hit_rate(hits: u64, n: u64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    hits as f64 / n as f64
}

/// Standard deviations from chance. The sign matters: the ladder is symmetric and the low tail is
/// as much of a finding as the high one.
pub fn deviation(hits: u64, n: u64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let n = n as f64;
    let expected = n * CHANCE;
    let sd = (n * CHANCE * (1.0 - CHANCE)).sqrt();
    (hits as f64 - expected) / sd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 5e-4
    }

    /// The worked example in `contracts/http-api.md`: 21 hits in 120 trials.
    #[test]
    fn the_bound_matches_the_published_example() {
        let bound = wilson_lower(21, 120, WILSON_Z);
        assert!(close(bound, 0.117), "got {bound}");
    }

    /// D8's first measure: the bound is not the hit rate, and the gap is the penalty for a small
    /// sample. Four out of four is a rate of 100 % and a guaranteed minimum near 51 %; the same
    /// rate over four hundred trials is worth nearly twice that.
    ///
    /// Note what this does *not* claim. The bound alone does not keep a four-trial run off the
    /// board — at 51 % it would still lead one. FR-040's hundred-trial floor is what does that, and
    /// it is tested against the board itself. What the bound buys is that the ordering *above* the
    /// floor estimates ability rather than luck.
    #[test]
    fn a_small_sample_is_penalised_against_its_own_rate() {
        let short = wilson_lower(4, 4, WILSON_Z);
        let long = wilson_lower(400, 400, WILSON_Z);
        assert!(short < 0.6, "four perfect trials claimed {short}");
        assert!(long > 0.98, "four hundred perfect trials claimed {long}");
        assert!(short < long);
    }

    /// More evidence at the same rate can only raise the guaranteed minimum.
    #[test]
    fn the_bound_rises_with_evidence_at_a_fixed_rate() {
        let mut previous = 0.0;
        for n in [8u64, 80, 800, 8000] {
            let bound = wilson_lower(n / 4, n, WILSON_Z);
            assert!(bound > previous, "the bound fell at n = {n}");
            previous = bound;
        }
    }

    #[test]
    fn nothing_observed_yields_nothing_claimed() {
        assert_eq!(wilson_lower(0, 0, WILSON_Z), 0.0);
        assert_eq!(deviation(0, 0), 0.0);
        assert_eq!(wilson_lower(0, 100, WILSON_Z), 0.0);
    }

    /// Chance reads as zero deviation, and the two tails are mirror images. That symmetry is what
    /// D23's ladder rests on: under the null the Kartoffeln and the Annunaki arrive in equal
    /// numbers, and the ratio between them is a significance test anyone can read.
    #[test]
    fn the_two_tails_mirror_each_other() {
        assert!(close(deviation(100, 800), 0.0));
        let above = deviation(150, 800);
        let below = deviation(50, 800);
        assert!(above > 0.0 && below < 0.0);
        assert!(close(above, -below), "{above} against {below}");
    }
}
