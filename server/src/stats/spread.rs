//! The distribution of qualified accounts over sigma bands (FR-043).
//!
//! Two counts at the ends answer one question — are the tails equally heavy — and answer it with a
//! chart that reads as broken whenever both are zero, which under the null is the overwhelmingly
//! likely state for a long time. The site's whole claim is that it publishes the unflattering
//! number, so the number has to be *visible* rather than merely correct: the full spread puts every
//! qualified account somewhere, so a reader sees a measured population sitting on chance instead of
//! two empty columns.
//!
//! The bands are mirror-symmetric by construction, and that is load-bearing rather than tidy. The
//! side-by-side test FR-043 asks a reader to perform only works if the two sides are cut the same
//! way, so a band is chosen on `|deviation|` and the sign only picks the side. Plain half-open
//! binning over signed edges would not do it — it would put −2.0 two bands in from the left and
//! +2.0 two bands in from the right, which are not mirror images. Both tails are therefore exactly
//! the sum of the outer bands on their side, which is why the tail counts are derived here rather
//! than by a second query that could disagree with the picture.

use serde::Serialize;

/// Band edges in standard deviations, as absolute distances from chance.
///
/// One sigma per step out to three, then everything beyond. The outermost band has to stay
/// open-ended: an account can sit at nine sigma, and a chart that quietly dropped it would be the
/// one dishonesty this page cannot afford.
const EDGES: [f64; 3] = [1.0, 2.0, 3.0];

/// Where a tail starts, and so what "markedly" means on this page (R5).
///
/// An edge in [`EDGES`] rather than an independent figure: a tail that did not fall on a band
/// boundary could not be read off the chart, and the two numbers would drift apart the first time
/// one of them moved. [`the_tail_edge_is_a_band_edge`] holds them together.
pub const TAIL_SIGMA: f64 = 2.0;

/// Which side of chance a band sits on. The sign, kept separate from the distance so that the two
/// sides are cut by the same code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Low,
    High,
}

/// One column of the chart: a band and how many accounts are in it.
///
/// `from` and `to` are signed and ordered, so the client draws the columns left to right without
/// knowing how they were cut. `to` is absent on the lowest band and `from` on the highest, which is
/// how an open end is stated rather than implied by some large number.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Band {
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub accounts: u64,
    /// Whether this band is part of a tail, so the emphasis in the chart follows the server's
    /// definition of a tail instead of a threshold the client keeps its own copy of.
    pub tail: bool,
}

/// The whole spread, plus the two figures that are read off it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spread {
    /// Ordered from the most negative band to the most positive.
    pub bands: Vec<Band>,
    /// Accounts with a long enough record to appear at all. The denominator for every band, and the
    /// number that tells a reader a flat-looking chart is a small population and not a fault.
    pub qualified: u64,
    pub tail_high: u64,
    pub tail_low: u64,
}

/// The side and the step one deviation falls in.
///
/// `step` counts the edges the distance has reached, so step 0 is the innermost band and
/// `EDGES.len()` the open-ended one. Zero counts as high, which is arbitrary and has to be — a
/// value has to go somewhere and no float that is exactly zero can be split. It costs nothing: the
/// innermost bands are not tails, so an account at exactly chance reads the same on either side.
fn place(deviation: f64) -> (Side, usize) {
    let side = if deviation < 0.0 {
        Side::Low
    } else {
        Side::High
    };
    let distance = deviation.abs();
    (side, EDGES.iter().filter(|edge| distance >= **edge).count())
}

/// The band a `(side, step)` pair describes, as signed bounds.
fn bounds(side: Side, step: usize) -> (Option<f64>, Option<f64>) {
    let inner = if step == 0 { 0.0 } else { EDGES[step - 1] };
    let outer = EDGES.get(step).copied();
    match side {
        Side::Low => (outer.map(|edge| -edge), Some(-inner)),
        Side::High => (Some(inner), outer),
    }
}

/// The first step that counts as a tail: the one whose inner edge is [`TAIL_SIGMA`].
fn tail_step() -> usize {
    EDGES
        .iter()
        .position(|edge| *edge == TAIL_SIGMA)
        .expect("TAIL_SIGMA is one of EDGES")
        + 1
}

/// The spread over every qualified account's deviation.
pub fn of(deviations: &[f64]) -> Spread {
    let steps = EDGES.len();
    let tail_from = tail_step();

    // Most negative first: the negative side outward-in, then the positive side inward-out.
    let layout = (0..=steps)
        .rev()
        .map(|step| (Side::Low, step))
        .chain((0..=steps).map(|step| (Side::High, step)));

    let bands: Vec<Band> = layout
        .map(|(side, step)| {
            let (from, to) = bounds(side, step);
            Band {
                from,
                to,
                accounts: deviations
                    .iter()
                    .filter(|deviation| place(**deviation) == (side, step))
                    .count() as u64,
                tail: step >= tail_from,
            }
        })
        .collect();

    // The layout puts the whole negative side first, so the halves are a split and not a search.
    let (low, high) = bands.split_at(steps + 1);
    let tail = |side: &[Band]| side.iter().filter(|b| b.tail).map(|b| b.accounts).sum();

    Spread {
        qualified: deviations.len() as u64,
        tail_low: tail(low),
        tail_high: tail(high),
        bands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(spread: &Spread) -> Vec<(Option<f64>, Option<f64>)> {
        spread.bands.iter().map(|b| (b.from, b.to)).collect()
    }

    /// The chart is cut the same way on both sides, and both ends are open.
    #[test]
    fn the_bands_are_mirror_symmetric_and_open_at_the_ends() {
        let spread = of(&[]);
        assert_eq!(
            edges(&spread),
            vec![
                (None, Some(-3.0)),
                (Some(-3.0), Some(-2.0)),
                (Some(-2.0), Some(-1.0)),
                (Some(-1.0), Some(0.0)),
                (Some(0.0), Some(1.0)),
                (Some(1.0), Some(2.0)),
                (Some(2.0), Some(3.0)),
                (Some(3.0), None),
            ]
        );
        assert_eq!(
            spread.bands.iter().map(|b| b.tail).collect::<Vec<_>>(),
            vec![true, true, false, false, false, false, true, true]
        );
    }

    /// The two figures in the sentence and the emphasis in the chart come from one definition.
    #[test]
    fn the_tail_edge_is_a_band_edge() {
        assert_eq!(EDGES[tail_step() - 1], TAIL_SIGMA);
    }

    /// An empty population is still a full chart. This is the state the page will be in for a long
    /// while, and the one that used to look like a broken page.
    #[test]
    fn an_empty_population_still_reports_every_band() {
        let spread = of(&[]);
        assert_eq!(spread.bands.len(), 8);
        assert_eq!(spread.qualified, 0);
        assert!(spread.bands.iter().all(|b| b.accounts == 0));
        assert_eq!((spread.tail_low, spread.tail_high), (0, 0));
    }

    /// Every qualified account lands in exactly one band, including the ones sitting on chance that
    /// a pair of tail counts would never have shown.
    #[test]
    fn every_account_lands_in_exactly_one_band() {
        let spread = of(&[-4.5, -2.5, -0.24, 0.0, 0.9, 1.5, 2.5, 7.0]);
        assert_eq!(spread.qualified, 8);
        assert_eq!(
            spread.bands.iter().map(|b| b.accounts).collect::<Vec<_>>(),
            vec![1, 1, 0, 1, 2, 1, 1, 1]
        );
        assert_eq!(
            spread.bands.iter().map(|b| b.accounts).sum::<u64>(),
            spread.qualified
        );
    }

    /// The tails are the outer bands of each side and nothing else, so the counts in the sentence
    /// and the bars in the chart cannot disagree.
    #[test]
    fn the_tails_are_the_outer_bands_of_each_side() {
        let spread = of(&[-9.0, -2.2, -1.9, 0.5, 2.1, 3.3, 4.0]);
        assert_eq!(spread.tail_low, 2);
        assert_eq!(spread.tail_high, 3);
    }

    /// The boundary belongs to the tail on both sides. A definition that took +2.0 but not −2.0
    /// would break the side-by-side comparison the two counts exist for.
    #[test]
    fn the_boundary_is_a_tail_on_both_sides() {
        let spread = of(&[TAIL_SIGMA, -TAIL_SIGMA]);
        assert_eq!(spread.tail_high, 1);
        assert_eq!(spread.tail_low, 1);
    }
}
