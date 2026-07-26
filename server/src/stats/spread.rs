//! The distribution of qualified accounts over the rank bands (FR-043).
//!
//! Two counts at the ends answer one question — are the tails equally heavy — and answer it with a
//! chart that reads as broken whenever both are zero, which under the null is the overwhelmingly
//! likely state for a long time. The site's whole claim is that it publishes the unflattering
//! number, so the number has to be *visible* rather than merely correct: the full spread puts every
//! qualified account somewhere, so a reader sees a measured population sitting on chance instead of
//! two empty columns.
//!
//! The columns are the ladder. Since D31 a rank is a distance from chance rather than a share of
//! the population, so the band edges here and the rungs in [`crate::config::Thresholds::bands`] are
//! the same numbers read out of the same list — which is why this takes the thresholds rather than
//! keeping a copy. A chart with its own edges is how a chart starts disagreeing with the ladder
//! drawn underneath it, and that disagreement was the previous version's actual bug: eight columns
//! under eleven rungs, with no way to line one up against the other.
//!
//! The bands are mirror-symmetric by construction, and that is load-bearing rather than tidy. The
//! side-by-side test FR-043 asks a reader to perform only works if the two sides are cut the same
//! way, so a band is chosen on `|deviation|` and the sign only picks the side. Plain half-open
//! binning over signed edges would not do it — it would put −2.0 two bands in from the left and
//! +2.0 two bands in from the right, which are not mirror images. Both tails are therefore exactly
//! the sum of the outer bands on their side, which is why the tail counts are derived here rather
//! than by a second query that could disagree with the picture.
//!
//! Normie is **one** column and not two. It is one rung, and a middle split into a left and a right
//! half would put twelve columns under eleven rungs and reintroduce the mismatch by a smaller
//! margin.

use serde::Serialize;

use crate::config::Thresholds;

/// Where a tail starts, and so what "markedly" means on this page (R5).
///
/// A band edge rather than an independent figure: a tail that did not fall on a boundary could not
/// be read off the chart, and the two numbers would drift apart the first time one of them moved.
/// [`the_tail_edge_is_a_band_edge`] holds them together.
pub const TAIL_SIGMA: f64 = 1.9;

/// Which side of chance a band sits on. The sign, kept separate from the distance so that the two
/// sides are cut by the same code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Low,
    High,
}

/// One column of the chart: a band, the rung it is, and how many accounts are in it.
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
    /// The rank slug this column awards, or `None` for the middle — the same absence the `rank`
    /// field of an account uses for Normie. Published so the chart can label a column with the
    /// rung it is, without the client re-deriving the mapping from the edges and getting it
    /// half-right.
    pub rank: Option<String>,
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
/// `step` counts the edges the distance has reached, so step 0 is the middle band and `edges.len()`
/// the open-ended one. Zero counts as high, which is arbitrary and has to be — a value has to go
/// somewhere and no float that is exactly zero can be split. It costs nothing: step 0 is the middle
/// column whichever side it came from, so an account at exactly chance reads the same either way.
fn place(deviation: f64, edges: &[f64]) -> (Side, usize) {
    let side = if deviation < 0.0 {
        Side::Low
    } else {
        Side::High
    };
    let distance = deviation.abs();
    (side, edges.iter().filter(|edge| distance >= **edge).count())
}

/// The band a `(side, step)` pair describes, as signed bounds. Step 0 is the middle and spans both
/// sides of chance, so the side does not enter into it.
fn bounds(side: Side, step: usize, edges: &[f64]) -> (Option<f64>, Option<f64>) {
    if step == 0 {
        return (Some(-edges[0]), Some(edges[0]));
    }
    let inner = edges[step - 1];
    let outer = edges.get(step).copied();
    match side {
        Side::Low => (outer.map(|edge| -edge), Some(-inner)),
        Side::High => (Some(inner), outer),
    }
}

/// The slug a `(side, step)` pair awards, or `None` for the middle.
///
/// `t.bands` is best first and `edges` nearest first, so step `s` counting outward is the band `s`
/// from the end of the list. Read off the same list the edges came from rather than matched by
/// value: an operator who configures two rungs at the same edge should get a lopsided ladder, not a
/// silently mislabelled chart.
fn slug(side: Side, step: usize, t: &Thresholds) -> Option<String> {
    if step == 0 {
        return None;
    }
    let band = t.bands.get(t.bands.len() - step)?;
    Some(match side {
        Side::High => band.high.clone(),
        Side::Low => band.low.clone(),
    })
}

/// The first step that counts as a tail: the one whose inner edge is [`TAIL_SIGMA`].
///
/// Falls back to the open-ended step if no edge reaches it, so a reconfigured ladder narrows the
/// tails rather than panicking in a request handler.
fn tail_step(edges: &[f64]) -> usize {
    edges
        .iter()
        .position(|edge| *edge >= TAIL_SIGMA)
        .map_or(edges.len(), |index| index + 1)
}

/// The spread over every qualified account's deviation, cut on the ladder in force.
pub fn of(deviations: &[f64], t: &Thresholds) -> Spread {
    let edges = t.edges();
    let steps = edges.len();
    let tail_from = tail_step(&edges);

    // Most negative first: the negative side outward-in, then the middle once, then the positive
    // side inward-out. The middle is emitted by the low half and skipped by the high one, which is
    // what makes eleven rungs eleven columns.
    let layout = (1..=steps)
        .rev()
        .map(|step| (Side::Low, step))
        .chain(std::iter::once((Side::High, 0)))
        .chain((1..=steps).map(|step| (Side::High, step)));

    let bands: Vec<Band> = layout
        .map(|(side, step)| {
            let (from, to) = bounds(side, step, &edges);
            Band {
                from,
                to,
                // The middle column holds both signs, so it counts on the step alone. Every other
                // column is one side of one step.
                accounts: deviations
                    .iter()
                    .filter(|deviation| {
                        let at = place(**deviation, &edges);
                        if step == 0 {
                            at.1 == 0
                        } else {
                            at == (side, step)
                        }
                    })
                    .count() as u64,
                tail: step >= tail_from,
                rank: slug(side, step, t),
            }
        })
        .collect();

    // The layout puts the whole negative side first, so the halves are a split and not a search.
    // The middle is never a tail, so which half it lands in does not affect either count.
    let (low, high) = bands.split_at(steps);
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

    /// The chart is cut the same way on both sides, both ends are open, and the middle is a single
    /// column straddling chance.
    #[test]
    fn the_bands_are_mirror_symmetric_with_one_middle_and_open_ends() {
        let spread = of(&[], &Thresholds::default());
        assert_eq!(
            edges(&spread),
            vec![
                (None, Some(-3.5)),
                (Some(-3.5), Some(-2.7)),
                (Some(-2.7), Some(-1.9)),
                (Some(-1.9), Some(-1.1)),
                (Some(-1.1), Some(-0.3)),
                (Some(-0.3), Some(0.3)),
                (Some(0.3), Some(1.1)),
                (Some(1.1), Some(1.9)),
                (Some(1.9), Some(2.7)),
                (Some(2.7), Some(3.5)),
                (Some(3.5), None),
            ]
        );
        assert_eq!(
            spread.bands.iter().map(|b| b.tail).collect::<Vec<_>>(),
            vec![
                true, true, true, false, false, false, false, false, true, true, true
            ]
        );
    }

    /// One column per rung, in the ladder's order, with Normie unnamed in the middle. This is the
    /// property the rebuild exists for: the chart and the ladder are one axis.
    #[test]
    fn every_column_is_a_rung() {
        let t = Thresholds::default();
        let spread = of(&[], &t);
        assert_eq!(spread.bands.len(), 2 * t.bands.len() + 1);
        assert_eq!(
            spread
                .bands
                .iter()
                .map(|b| b.rank.as_deref())
                .collect::<Vec<_>>(),
            vec![
                Some("kartoffel"),
                Some("nullleiter"),
                Some("orgonit"),
                Some("erdstrahlen"),
                Some("pineal"),
                None,
                Some("asset"),
                Some("grey"),
                Some("reptilian"),
                Some("loosh"),
                Some("annunaki"),
            ]
        );
    }

    /// The column a deviation lands in and the rank it is awarded are the same decision, so the
    /// chart cannot put an account in a column whose title it does not hold.
    #[test]
    fn a_columns_rank_is_the_rank_that_deviation_earns() {
        let t = Thresholds::default();
        for step in -500i32..=500 {
            let z = f64::from(step) * 0.01;
            let spread = of(&[z], &t);
            let column = spread
                .bands
                .iter()
                .find(|b| b.accounts == 1)
                .expect("one account is in exactly one column");
            let awarded = crate::stats::ranks::band_for(z, &t).map(|a| a.slug().to_owned());
            assert_eq!(column.rank, awarded, "at {z} σ");
        }
    }

    /// The two figures in the sentence and the emphasis in the chart come from one definition.
    #[test]
    fn the_tail_edge_is_a_band_edge() {
        let edges = Thresholds::default().edges();
        assert_eq!(edges[tail_step(&edges) - 1], TAIL_SIGMA);
    }

    /// An empty population is still a full chart. This is the state the page will be in for a long
    /// while, and the one that used to look like a broken page.
    #[test]
    fn an_empty_population_still_reports_every_band() {
        let spread = of(&[], &Thresholds::default());
        assert_eq!(spread.bands.len(), 11);
        assert_eq!(spread.qualified, 0);
        assert!(spread.bands.iter().all(|b| b.accounts == 0));
        assert_eq!((spread.tail_low, spread.tail_high), (0, 0));
    }

    /// Every qualified account lands in exactly one band, including the ones sitting on chance that
    /// a pair of tail counts would never have shown.
    #[test]
    fn every_account_lands_in_exactly_one_band() {
        let spread = of(
            &[-4.5, -2.5, -0.24, 0.0, 0.9, 1.5, 2.5, 7.0],
            &Thresholds::default(),
        );
        assert_eq!(spread.qualified, 8);
        assert_eq!(
            spread.bands.iter().map(|b| b.accounts).collect::<Vec<_>>(),
            vec![1, 0, 1, 0, 0, 2, 1, 1, 1, 0, 1]
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
        let spread = of(
            &[-9.0, -2.2, -1.8, 0.5, 2.1, 3.3, 4.0],
            &Thresholds::default(),
        );
        assert_eq!(spread.tail_low, 2);
        assert_eq!(spread.tail_high, 3);
    }

    /// The boundary belongs to the tail on both sides. A definition that took +1.9 but not −1.9
    /// would break the side-by-side comparison the two counts exist for.
    #[test]
    fn the_boundary_is_a_tail_on_both_sides() {
        let spread = of(&[TAIL_SIGMA, -TAIL_SIGMA], &Thresholds::default());
        assert_eq!(spread.tail_high, 1);
        assert_eq!(spread.tail_low, 1);
    }

    /// Chance itself is the middle column, not a tail and not a side.
    #[test]
    fn an_account_on_chance_sits_in_the_middle() {
        let spread = of(&[0.0], &Thresholds::default());
        let middle = &spread.bands[5];
        assert_eq!(middle.accounts, 1);
        assert_eq!(middle.rank, None);
        assert!(!middle.tail);
        assert_eq!((spread.tail_low, spread.tail_high), (0, 0));
    }
}
