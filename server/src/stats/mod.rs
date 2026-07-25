//! Statistics — the part of the product that has to be right even when it is unflattering.
//!
//! D18 states the premise: at 100,000 trials the 95 % interval is ±0.21 %, so the site will almost
//! certainly display 12.50 % and the premise it exists to test will fail. That is the result, not
//! a failure, and every rule in here follows from allowing it to be. In particular D8's rule —
//! **gate the display, never the data**: the aggregate runs over every trial by every account,
//! including those of users who never saw a statistics page, because conditioning the population
//! on success is what makes a psi site overstate the very number it reports.

pub mod accumulate;
pub mod blocks;
pub mod by_chance;
pub mod eligibility;
pub mod measures;
pub mod ranks;

/// One account's figures, as `GET /api/stats/me` reports them.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AccountStats {
    pub completed: u64,
    pub hits: u64,
    /// Always present, so selective abandonment is visible rather than hidden (FR-021).
    pub abandoned: u64,
    pub distinct_utc_days: u32,
    pub wilson_lower: f64,
    /// The highest rate the record still admits. The interpretable figure for a low-tail account,
    /// whose lower bound is zero however many trials it has behind it.
    pub wilson_upper: f64,
    /// Standard deviations from chance.
    pub deviation: f64,
    pub eligible: bool,
    /// The band slug, absent while the population is too small for the band to exist (D23).
    pub rank: Option<String>,
}
