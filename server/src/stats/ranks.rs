//! Share-based ranks (D23, FR-042).
//!
//! Bands are shares, not seats. D19 was right that supply must be fixed rather than earned by
//! hitting a threshold — absolute thresholds would have minted twenty-eight archons on a
//! thousand-visitor launch day — and wrong to fix it as a seat count: third place out of ten is
//! nothing, third place out of two hundred thousand is a title. A share means the same thing at
//! every population, which is the only form that survives the site growing.

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

/// The band a place on the board falls into, or `None` for the middle 60 % — Normie, and the
/// honest answer for almost everyone.
///
/// A band exists only once `share * eligible >= 1`, with **no rounding up**: rounding would hand
/// out the rarest title at any population, which is the opposite of what a share means. The top
/// rung is therefore unreachable until a thousand people have taken this seriously.
///
/// The bands are nested — the best 0,1 % is inside the best 0,5 % — so the first match walking from
/// the narrowest outward is the one to award.
pub fn band_for<'t>(place: u64, eligible: u64, t: &'t Thresholds) -> Option<Awarded<'t>> {
    if place == 0 || place > eligible {
        return None;
    }
    for band in &t.bands {
        if place <= holders(band, eligible) {
            return Some(Awarded {
                band,
                side: Side::High,
            });
        }
    }
    // Counted from the bottom, against the same cut. The two ends can never overlap: every share
    // is below a half, so the widest band's two halves together hold less than the population.
    let from_bottom = eligible - place + 1;
    for band in &t.bands {
        if from_bottom <= holders(band, eligible) {
            return Some(Awarded {
                band,
                side: Side::Low,
            });
        }
    }
    None
}

/// How many accounts a band holds at this population, at each end. Zero means the band does not
/// exist yet.
pub fn holders(band: &RankBand, eligible: u64) -> u64 {
    (band.share * eligible as f64).floor() as u64
}

/// The bands that currently exist, widest first, named by their upper slug (D23, FR-042).
///
/// The board reports this whether or not it is empty, so it can say how far off the next rung is
/// rather than leaving a reader to wonder why nobody has a title.
pub fn active(eligible: u64, t: &Thresholds) -> Vec<&str> {
    t.bands
        .iter()
        .rev()
        .filter(|b| holders(b, eligible) >= 1)
        .map(|b| b.high.as_str())
        .collect()
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
    let ordered: Vec<String> = {
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
    let eligible = ordered.len() as u64;

    db.write(|tx| {
        // Cleared wholesale first. An account that fell out of eligibility — the floor moved, or
        // the operator raised it (D26) — keeps its title otherwise, and SC-013 says no account
        // holds a rank without meeting the rule in force.
        tx.execute(
            "UPDATE account_stats SET eligible = 0, rank_slug = NULL, ranked_at = ?1",
            params![now],
        )?;
        for (index, account_id) in ordered.iter().enumerate() {
            let place = index as u64 + 1;
            let slug = band_for(place, eligible, t).map(|a| a.slug());
            tx.execute(
                "UPDATE account_stats SET eligible = 1, rank_slug = ?2, ranked_at = ?3
                  WHERE account_id = ?1",
                params![account_id, slug, now],
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

    fn slug_at(place: u64, eligible: u64, t: &Thresholds) -> Option<&str> {
        band_for(place, eligible, t).map(|a| a.slug())
    }

    /// D23's rule, at the exact populations `Thresholds::band_unlocks_at` names. One account short
    /// of each, the band must not exist — that is what "no rounding up" means, and it is the whole
    /// reason the top rung is a joke about a thousand people rather than a prize for showing up.
    #[test]
    fn a_band_appears_only_when_a_share_of_the_population_reaches_one() {
        let t = Thresholds::default();
        for band in &t.bands {
            let at = t.band_unlocks_at(band);
            assert_eq!(
                slug_at(1, at, &t),
                Some(band.high.as_str()),
                "{} should exist at {at} eligible",
                band.high
            );
            assert_ne!(
                slug_at(1, at - 1, &t),
                Some(band.high.as_str()),
                "{} was handed out at {} eligible",
                band.high,
                at - 1
            );
        }
    }

    /// Below the widest band's threshold there are no titles at all — five eligible accounts is not
    /// a leaderboard, and D23 would rather say so than crown the least unlucky of four.
    #[test]
    fn a_population_too_small_for_any_band_has_no_titles() {
        let t = Thresholds::default();
        for eligible in 1..5u64 {
            for place in 1..=eligible {
                assert_eq!(slug_at(place, eligible, &t), None);
            }
            assert!(active(eligible, &t).is_empty());
        }
    }

    /// The ladder is symmetric, so every title has a mirror at the same distance from the other end
    /// — the property that makes the ratio between the two tails a significance test a reader
    /// performs by looking (D19's closing argument, carried into D23).
    #[test]
    fn every_place_has_its_mirror_at_the_other_end() {
        let t = Thresholds::default();
        for eligible in [5u64, 15, 50, 200, 1000, 4321] {
            for place in 1..=eligible.min(60) {
                let top = band_for(place, eligible, &t);
                let bottom = band_for(eligible - place + 1, eligible, &t);
                match (top, bottom) {
                    (Some(a), Some(b)) => {
                        assert_eq!(a.band, b.band, "different rungs at {place} of {eligible}");
                        assert_ne!(a.side, b.side, "the same end twice");
                    }
                    (None, None) => {}
                    other => panic!("the ladder is lopsided at {place} of {eligible}: {other:?}"),
                }
            }
        }
    }

    /// The middle 60 % is Normie and gets no slug at all. At a thousand eligible the ladder holds
    /// 20 % each side, so places 201 through 800 are the honest majority.
    #[test]
    fn the_middle_is_normie() {
        let t = Thresholds::default();
        let eligible = 1000;
        assert_eq!(slug_at(200, eligible, &t), Some("asset"));
        assert_eq!(slug_at(201, eligible, &t), None);
        assert_eq!(slug_at(800, eligible, &t), None);
        assert_eq!(slug_at(801, eligible, &t), Some("pineal"));
    }

    /// The two ends can never claim the same place, whatever the population.
    #[test]
    fn the_bands_never_overlap() {
        let t = Thresholds::default();
        for eligible in 1..300u64 {
            let held: u64 = t.bands.iter().map(|b| 2 * holders(b, eligible)).sum();
            assert!(
                held <= eligible,
                "{held} titles for {eligible} eligible accounts"
            );
        }
    }

    /// The worked example in `contracts/http-api.md`: at 214 eligible the ladder has filled in
    /// from the middle outward as far as the best 0,5 %, and no further.
    #[test]
    fn the_active_bands_fill_in_from_the_middle_outward() {
        let t = Thresholds::default();
        assert_eq!(
            active(214, &t),
            vec!["asset", "grey", "reptilian", "loosh"],
            "annunaki needs a thousand"
        );
        assert_eq!(active(1000, &t).len(), 5);
    }

    #[test]
    fn a_place_outside_the_population_has_no_band() {
        let t = Thresholds::default();
        assert_eq!(band_for(0, 1000, &t), None);
        assert_eq!(band_for(1001, 1000, &t), None);
    }
}
