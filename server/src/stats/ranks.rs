//! Deviation-based ranks (D31, superseding D23).
//!
//! A rank is a distance from chance, not a place in a queue. Shares were the previous answer and
//! they were wrong for this site in three ways. A share makes your title depend on who else signed
//! up, so a player who did nothing can be demoted by strangers. A share cannot be checked: it needs
//! the whole population and the server's sort, on a site whose entire claim is that a visitor can
//! verify the numbers from their own record. And a share made the ladder's symmetry true by
//! definition — the two tails held equal shares because they were configured to, so the
//! tail-versus-tail comparison FR-043 asks a reader to perform proved nothing at all.
//!
//! On sigma edges the same comparison becomes an empirical result: equal counts at the two ends
//! means chance, a heavier top means something to explain. What is given up is supply control —
//! titles now grow linearly with the population instead of being capped — which is the correct
//! trade for a measurement, and the eligibility rule (100 trials across 3 days) remains the brake
//! on grinding one out.

use rusqlite::{Connection, OptionalExtension, params};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::config::{Config, RankBand, Thresholds};
use crate::db::{Db, DbError};
use crate::stats::accumulate;

/// How stale a rank may be before a read pays for a fresh pass (D23).
///
/// Fifteen minutes is the interval D23 names, and the board publishes when the last pass ran, so a
/// rank that has not moved reads as a rank rather than as a bug.
pub const RECOMPUTE_AFTER_SECONDS: i64 = 15 * 60;

/// The board's order, and the same order [`recompute`] numbers places in.
///
/// One string rather than two identical `ORDER BY` clauses: places assigned in one order and rows
/// listed in another would put the account holding place 1 somewhere in the middle of page one, and
/// nothing about that failure looks like a bug until somebody counts.
///
/// The Wilson bound is the sort key D20 settled on, and the upper bound is the second one rather
/// than a tie-break of convenience. Read as a pair the order is "best guaranteed floor first, then
/// best ceiling" — which at the top of the board is a refinement almost nothing reaches, and at the
/// bottom is the entire ordering. Below chance the lower bound is exactly zero at every `n`
/// (`measures::wilson_upper` says why), so the low tail ties on the first column and the second one
/// decides it: descending, the smallest ceiling sorts last, and the smallest ceiling is the most
/// evidence of an anti-talent. Ordering that tie by `completed` instead inverted the tail — three
/// hundred trials without a hit placed *above* a hundred without one, and the weaker result took
/// the Kartoffel.
///
/// The remaining tie-breaks exist only to make the order total: the longer record goes first, and
/// the public identifier settles the rest so two identical records do not swap places between
/// passes.
pub const BOARD_ORDER: &str =
    "s.wilson_lower DESC, s.wilson_upper DESC, s.completed DESC, a.public_id ASC";

/// A place's band, and which end of the ladder it fell on.
///
/// The side has to travel with the band: a [`RankBand`] holds both slugs precisely so the ladder
/// cannot drift out of symmetry, which means the band alone never says whether this account is an
/// Annunaki or a Kartoffel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    High,
    Low,
}

/// A band awarded to a place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Awarded<'a> {
    pub band: &'a RankBand,
    pub side: Side,
}

impl<'a> Awarded<'a> {
    /// The slug this place is published under. A slug, not a title: the titles are product copy and
    /// live in the client's message catalogue, one per domain.
    pub fn slug(&self) -> &'a str {
        match self.side {
            Side::High => &self.band.high,
            Side::Low => &self.band.low,
        }
    }
}

/// The band a deviation falls into, or `None` for the middle — Normie, and the honest answer for
/// about a quarter of everyone.
///
/// The distance decides the rung and the sign decides the end, which is the whole of the symmetry:
/// one comparison, applied to `|deviation|`, cannot cut the two sides differently. Bands are listed
/// best first, so the first edge the distance clears is the one to award.
///
/// Zero counts as high, which is arbitrary and has to be — a value has to go somewhere and no float
/// that is exactly zero can be split. It costs nothing: a deviation of zero is Normie on either
/// side, and Normie has no slug.
pub fn band_for<'t>(deviation: f64, t: &'t Thresholds) -> Option<Awarded<'t>> {
    let side = if deviation < 0.0 {
        Side::Low
    } else {
        Side::High
    };
    let distance = deviation.abs();
    t.bands
        .iter()
        .find(|band| distance >= band.from_sigma)
        .map(|band| Awarded { band, side })
}

/// The whole ladder, nearest the middle first, named by its upper slug (FR-042).
///
/// Every band exists at every population now that a rung is a distance rather than a share, so this
/// is the full list and no longer a function of how many people have played. It stays on the board
/// because the board's readers use it to see what the rungs above them are called; the population
/// argument it used to carry is gone with D23.
pub fn active(t: &Thresholds) -> Vec<&str> {
    t.bands.iter().rev().map(|b| b.high.as_str()).collect()
}

/// Recomputes and materialises every rank, returning the number of eligible accounts.
///
/// Runs every ~15 minutes rather than per request or per block, so the board is a read of one
/// table and stays stable between passes. The board states when this last ran — a rank that has
/// not moved otherwise reads as a bug.
///
/// Note what this deliberately does **not** touch: `wilson_lower` and `deviation`. Those advance
/// per block of completed trials (FR-019) and recomputing them here on the current totals would
/// move every account's figure every fifteen minutes, which is exactly the mid-block movement the
/// block rule exists to prevent.
pub fn recompute(db: &Db, t: &Thresholds, now: &str) -> Result<u64, DbError> {
    // Two populations, and keeping them apart is the point (D33). A **title** is a statement about
    // one account's own record, so it needs only enough trials for a deviation to mean anything —
    // the same gate the statistics view and the distribution chart use, `stats_unlock_at`. The
    // **board** is a public ranking, and that is what the harder rule of 100 trials across three
    // days is for: it is a brake on grinding a place out, not on learning where you stand.
    let ranked_rows: Vec<(String, f64)> = {
        let reader = db.reader()?;
        let mut stmt = reader
            .prepare("SELECT account_id, deviation FROM account_stats WHERE completed >= ?1")?;
        let rows = stmt.query_map(params![t.stats_unlock_at], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<(String, f64)>>>()?
    };

    // Ordered by the board's order even though no rank depends on it. The order is not wasted: it
    // is the one the board reads back, and running the pass over the same query keeps "eligible"
    // meaning one thing in both places.
    let eligible_ids: Vec<String> = {
        let reader = db.reader()?;
        let mut stmt = reader.prepare(&format!(
            "SELECT s.account_id
               FROM account_stats s JOIN account a ON a.id = s.account_id
              WHERE s.completed >= ?1 AND s.distinct_utc_days >= ?2
              ORDER BY {BOARD_ORDER}"
        ))?;
        let rows = stmt.query_map(params![t.eligibility_trials, t.eligibility_days], |r| {
            r.get(0)
        })?;
        rows.collect::<rusqlite::Result<Vec<String>>>()?
    };
    let eligible = eligible_ids.len() as u64;

    db.write(|tx| {
        // Cleared wholesale first. An account that fell out of either rule — its own record shrank
        // by an expiry, or the operator moved a floor (D26) — keeps neither flag nor title
        // otherwise, and SC-013 says no account holds a rank without meeting the rule in force.
        tx.execute(
            "UPDATE account_stats SET eligible = 0, rank_slug = NULL, ranked_at = ?1",
            params![now],
        )?;
        for (account_id, deviation) in &ranked_rows {
            let slug = band_for(*deviation, t).map(|a| a.slug());
            tx.execute(
                "UPDATE account_stats SET rank_slug = ?2, ranked_at = ?3 WHERE account_id = ?1",
                params![account_id, slug, now],
            )?;
        }
        for account_id in &eligible_ids {
            tx.execute(
                "UPDATE account_stats SET eligible = 1 WHERE account_id = ?1",
                params![account_id],
            )?;
        }
        Ok(())
    })?;
    Ok(eligible)
}

/// When the ranks were last recomputed, or `None` on a database where no pass has run.
pub fn last_computed(conn: &Connection) -> Result<Option<String>, DbError> {
    let at = conn
        .query_row("SELECT MAX(ranked_at) FROM account_stats", [], |r| r.get(0))
        .optional()?
        .flatten();
    Ok(at)
}

/// Runs a pass if the last one has aged out, and refreshes any statistics row the log has moved
/// past first.
///
/// **This is a stand-in for T102.** D23 puts the pass on a fifteen-minute background timer; until
/// that task lands, the two endpoints that read ranks trigger it from the read side, gated on the
/// same interval so a public `GET` cannot be made to sweep the table on every request. When T102
/// arrives, the calls to this go and the timer calls [`recompute`] directly.
pub fn ensure_fresh(db: &Db, cfg: &Config, now: &str) -> Result<(), DbError> {
    let last = {
        let reader = db.reader()?;
        last_computed(&reader)?
    };
    if let Some(last) = last
        && last.as_str() > cutoff(now).as_str()
    {
        return Ok(());
    }
    accumulate::refresh_stale(db, cfg, now)?;
    recompute(db, &cfg.thresholds, now)?;
    Ok(())
}

/// The oldest pass timestamp that still counts as fresh.
///
/// Compared as strings, which RFC 3339 in UTC to whole seconds makes a correct comparison — the
/// same property the log's timestamp columns are ordered by.
fn cutoff(now: &str) -> String {
    let parsed = OffsetDateTime::parse(now, &Rfc3339).expect("a timestamp this process formatted");
    (parsed - time::Duration::seconds(RECOMPUTE_AFTER_SECONDS))
        .format(&Rfc3339)
        .expect("an RFC 3339 timestamp formats")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slug_at(deviation: f64, t: &Thresholds) -> Option<&str> {
        band_for(deviation, t).map(|a| a.slug())
    }

    /// Every rung, at its own edge and just inside it. The edge belongs to the band above, so a
    /// player who has just reached 1,9 σ is a Reptiloidenarchont and one at 1,89 σ is not.
    #[test]
    fn a_band_starts_exactly_at_its_edge() {
        let t = Thresholds::default();
        for band in &t.bands {
            let at = band.from_sigma;
            assert_eq!(slug_at(at, &t), Some(band.high.as_str()), "at +{at} σ");
            assert_eq!(slug_at(-at, &t), Some(band.low.as_str()), "at −{at} σ");
            assert_ne!(
                slug_at(at - 0.01, &t),
                Some(band.high.as_str()),
                "{} was handed out below its edge",
                band.high
            );
        }
    }

    /// The rank is the account's own figure and nothing else. Population appears nowhere in the
    /// signature, which is the whole of D31: nobody else signing up can move your title.
    #[test]
    fn the_ladder_is_the_same_at_every_population() {
        let t = Thresholds::default();
        assert_eq!(slug_at(2.0, &t), Some("reptilian"));
        assert_eq!(
            active(&t),
            vec!["asset", "grey", "reptilian", "loosh", "annunaki"]
        );
    }

    /// Distance decides the rung and the sign only decides the end, so every title has its mirror
    /// at the same distance below chance. That symmetry is what makes the ratio between the two
    /// tails a significance test a reader performs by looking (FR-043).
    #[test]
    fn every_deviation_has_its_mirror_at_the_other_end() {
        let t = Thresholds::default();
        for step in 0..500 {
            let z = step as f64 * 0.01;
            match (band_for(z, &t), band_for(-z, &t)) {
                (Some(a), Some(b)) => {
                    assert_eq!(a.band, b.band, "different rungs at {z} σ");
                    // Zero is the one deviation that cannot be split, and it is Normie on both
                    // sides, so it never reaches here with two bands.
                    assert_ne!(a.side, b.side, "the same end twice at {z} σ");
                }
                (None, None) => {}
                other => panic!("the ladder is lopsided at {z} σ: {other:?}"),
            }
        }
    }

    /// Inside ±0,3 σ there is no band at all — Normie, which under chance is about a quarter of
    /// everyone and the honest answer for them.
    #[test]
    fn the_middle_is_normie() {
        let t = Thresholds::default();
        for z in [-0.29, -0.1, 0.0, 0.1, 0.29] {
            assert_eq!(slug_at(z, &t), None, "at {z} σ");
        }
        assert_eq!(slug_at(0.3, &t), Some("asset"));
        assert_eq!(slug_at(-0.3, &t), Some("pineal"));
    }

    /// The top rung is open-ended. An account can sit at nine sigma, and the band that catches it
    /// has to be the best one rather than none at all.
    #[test]
    fn the_outermost_rungs_are_open_ended() {
        let t = Thresholds::default();
        assert_eq!(slug_at(9.0, &t), Some("annunaki"));
        assert_eq!(slug_at(-9.0, &t), Some("kartoffel"));
    }
}
