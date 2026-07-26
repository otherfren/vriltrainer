//! The file a third party downloads, per `contracts/public-log.md`.
//!
//! Newline-delimited JSON, one entry per line, ordered by `seq` with no gaps.
//!
//! The module is deliberately thin. Lines are [`Entry`]'s own serialisation and not a second
//! presentation shape maintained here: a display type would be a third place the record's field
//! names live, and the day it drifts from the body that goes into the hash, every downloaded copy
//! verifies against nothing while both halves still look correct.
//!
//! What makes the file worth downloading is what is *not* filtered out of it. An abandoned trial
//! is a commit with no resolve (see [`crate::log::chain`]), so leaving abandoned trials in is not
//! a courtesy — it is the only thing that makes selective abandonment countable by a stranger
//! rather than reportable by the operator (FR-027, SC-012). There is no "hide" path in here, and
//! there should never be one.

use std::collections::HashSet;

use crate::log::chain::{Body, Entry};

/// Entries per page when the caller does not say.
///
/// A whole-log download is a loop over pages, and the loop is safe to run because entries are
/// immutable once written: a page already fetched cannot change under a downloader who comes back
/// tomorrow for the rest.
pub const DEFAULT_LIMIT: u64 = 1_000;

/// The largest page anyone may ask for.
///
/// The log grows without bound, so an unbounded `limit` is a request that eventually holds the
/// whole record in memory to answer one GET. The cap costs a downloader nothing but another
/// request; its absence costs the process.
pub const MAX_LIMIT: u64 = 10_000;

/// The page size to actually use, from what the caller asked for.
///
/// `limit=0` is clamped up rather than honoured. Answering it literally returns an empty page for
/// every `from`, which a paging downloader reads as "the log ends here" — a silent truncation of
/// the one file the product asks strangers to keep copies of.
pub fn page_limit(asked: Option<u64>) -> u64 {
    asked.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Serialises entries as newline-delimited JSON, trailing newline included.
///
/// The trailing newline is there so that concatenating two pages in the obvious way produces a
/// valid file rather than one fused line straddling the join.
pub fn to_ndjson(entries: &[Entry]) -> String {
    let mut out = String::new();
    for e in entries {
        // An `Entry` is strings, integers and a bool. There is no map with non-string keys and no
        // float, which are the only shapes `to_string` refuses, so a failure here would mean the
        // type changed into something the export cannot represent — a bug, not a condition.
        out.push_str(&serde_json::to_string(e).expect("a log entry serialises"));
        out.push('\n');
    }
    out
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("line {line} is not a log entry: {message}")]
    Malformed { line: usize, message: String },
}

/// Reads an export back, which is the first thing any verifier does with one.
///
/// It lives beside the writer so the two are tested against each other. A format the project can
/// only produce is a format nobody has checked is readable, and "readable by someone else" is the
/// entire claim (SC-002).
///
/// Blank lines are skipped: files get concatenated, trimmed and passed through editors on the way
/// to a verifier, and refusing an empty line would fail a copy whose entries are all intact.
pub fn read_ndjson(text: &str) -> Result<Vec<Entry>, ExportError> {
    let mut entries = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        entries.push(
            serde_json::from_str(line).map_err(|e| ExportError::Malformed {
                line: i + 1,
                message: e.to_string(),
            })?,
        );
    }
    Ok(entries)
}

/// The figures a reader recomputes from the file, with nothing but the file.
///
/// This is a second implementation of numbers the server also reports from SQL, and that is the
/// point rather than an oversight: the headline hit rate is only worth publishing if an outsider
/// arrives at it independently (SC-004), and the abandonment rate is only worth publishing if it
/// is counted from the record rather than from a counter the operator maintains (FR-027, SC-012).
/// The two must agree; the moment they do not, one of them is wrong and the disagreement is the
/// finding.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Audit {
    pub commits: u64,
    pub resolves: u64,
    pub hits: u64,
    /// Commits with no resolve — abandoned trials.
    pub abandoned: u64,
    /// Distinct accounts appearing in commit entries.
    pub accounts: u64,
    /// Resolves whose commit is not in the range that was audited.
    ///
    /// Zero on a complete export. Anything else means the file starts partway into the log, and
    /// on such a file `abandoned` counts the commits that happen to be inside the window rather
    /// than the abandoned trials — see [`Audit::covers_whole_log`].
    pub orphan_resolves: u64,
}

impl Audit {
    /// Hits over resolves, or `None` before any trial has been answered.
    ///
    /// `None` rather than zero: a site with no trials has no hit rate, and rendering one as 0 %
    /// would be the first published figure and a false one.
    pub fn hit_rate(&self) -> Option<f64> {
        (self.resolves > 0).then(|| self.hits as f64 / self.resolves as f64)
    }

    /// Whether `abandoned` means what it says.
    ///
    /// A resolve without its commit can only happen when the export begins after that commit, so
    /// this is how a reader of a partial file finds out that the abandonment count in front of
    /// them is a window statistic and not the rate.
    pub fn covers_whole_log(&self) -> bool {
        self.orphan_resolves == 0
    }
}

/// Recomputes [`Audit`] from exported entries.
pub fn audit(entries: &[Entry]) -> Audit {
    let committed: HashSet<&str> = entries
        .iter()
        .filter(|e| matches!(e.body, Body::Commit { .. }))
        .map(|e| e.body.trial())
        .collect();
    let resolved: HashSet<&str> = entries
        .iter()
        .filter(|e| matches!(e.body, Body::Resolve { .. }))
        .map(|e| e.body.trial())
        .collect();

    let mut accounts: HashSet<&str> = HashSet::new();
    let mut out = Audit::default();
    for e in entries {
        match &e.body {
            Body::Commit { account, .. } => {
                out.commits += 1;
                accounts.insert(account);
            }
            Body::Resolve { hit, .. } => {
                out.resolves += 1;
                out.hits += u64::from(*hit);
            }
        }
    }

    out.accounts = accounts.len() as u64;
    out.abandoned = committed.iter().filter(|t| !resolved.contains(*t)).count() as u64;
    out.orphan_resolves = resolved.iter().filter(|t| !committed.contains(*t)).count() as u64;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::chain::{self, Chain};

    fn commit(trial: &str, account: &str) -> Body {
        Body::Commit {
            trial: trial.into(),
            account: account.into(),
            coordinate: "4821-9037".into(),
            commitment: "sha256:aa".into(),
            pool_version: 1,
            pool_manifest_hash: Some("sha256:pool".into()),
        }
    }

    fn resolve(trial: &str, hit: bool) -> Body {
        Body::Resolve {
            trial: trial.into(),
            chosen: "img_1".into(),
            target: if hit { "img_1" } else { "img_2" }.into(),
            hit,
            s_server: "c2VydmVy".into(),
            s_client: "Y2xpZW50".into(),
            nonce: "bm9uY2U=".into(),
        }
    }

    /// Two accounts, four trials: one hit, one miss, two abandoned.
    fn sample() -> Chain {
        let mut c = Chain::new();
        c.append("2026-07-25T10:00:00Z", commit("t1", "a1"));
        c.append("2026-07-25T10:00:01Z", commit("t2", "a1"));
        c.append("2026-07-25T10:00:02Z", commit("t3", "a2"));
        c.append("2026-07-25T10:00:03Z", commit("t4", "a2"));
        c.append("2026-07-25T10:00:09Z", resolve("t1", true));
        c.append("2026-07-25T10:00:12Z", resolve("t3", false));
        c
    }

    /// The round trip is the contract: whatever this project writes, a verifier must be able to
    /// read back into entries that hash to the same chain.
    #[test]
    fn an_export_reads_back_and_still_verifies() {
        let c = sample();
        let text = to_ndjson(c.entries());
        assert_eq!(text.lines().count(), c.len());
        assert!(text.ends_with('\n'), "pages must concatenate cleanly");

        let back = read_ndjson(&text).unwrap();
        assert_eq!(back, c.entries());
        assert_eq!(chain::verify(&back), Ok(()));
    }

    /// The field the export exists to carry (T061). Without `s_client` in the resolve entry only
    /// the participant's own browser can re-derive the decoys, and SC-002 promises anyone can.
    #[test]
    fn a_resolve_line_carries_both_randomness_contributions() {
        let c = sample();
        let line = to_ndjson(c.entries())
            .lines()
            .find(|l| l.contains("resolve"))
            .expect("the sample resolves two trials")
            .to_string();
        let json: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(json["s_server"], "c2VydmVy");
        assert_eq!(
            json["s_client"], "Y2xpZW50",
            "a resolve without s_client is only verifiable by the participant"
        );
        assert_eq!(json["nonce"], "bm9uY2U=");
    }

    /// FR-027 and SC-012, on the file rather than in the database.
    #[test]
    fn abandoned_trials_are_commits_without_resolves() {
        let entries = read_ndjson(&to_ndjson(sample().entries())).unwrap();
        let a = audit(&entries);
        assert_eq!(a.commits, 4);
        assert_eq!(a.resolves, 2);
        assert_eq!(a.hits, 1);
        assert_eq!(a.abandoned, 2, "t2 and t4 were never answered");
        assert_eq!(a.accounts, 2);
        assert_eq!(a.hit_rate(), Some(0.5));
        assert!(a.covers_whole_log());
    }

    /// A partial download says so, rather than reporting an abandonment count that is really a
    /// count of whichever commits fell inside the window.
    #[test]
    fn a_range_that_starts_late_is_visible_as_partial() {
        let c = sample();
        let a = audit(&c.entries()[4..]);
        assert_eq!(a.orphan_resolves, 2);
        assert!(!a.covers_whole_log());
    }

    #[test]
    fn an_empty_log_has_no_hit_rate_rather_than_a_rate_of_zero() {
        assert_eq!(audit(&[]).hit_rate(), None);
    }

    #[test]
    fn a_page_limit_is_clamped_into_something_answerable() {
        assert_eq!(page_limit(None), DEFAULT_LIMIT);
        assert_eq!(page_limit(Some(7)), 7);
        assert_eq!(page_limit(Some(u64::MAX)), MAX_LIMIT);
        assert_eq!(page_limit(Some(0)), 1, "an empty page reads as the end");
    }

    #[test]
    fn a_damaged_line_is_named() {
        let mut text = to_ndjson(sample().entries());
        text.push_str("{ not json }\n");
        match read_ndjson(&text) {
            Err(ExportError::Malformed { line, .. }) => assert_eq!(line, 7),
            other => panic!("expected a malformed line, got {other:?}"),
        }
    }
}
