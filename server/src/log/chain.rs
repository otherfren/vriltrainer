//! The append-only hash chain, per `contracts/public-log.md`.
//!
//! A trial changes state — created, revealed, answered — but an append-only chain cannot have its
//! entries edited. Mutating a row on answer would silently make the record rewritable, which is
//! the property D2 exists to prevent. So each trial contributes one `Commit` entry and at most one
//! `Resolve` entry, and **an abandoned trial is a commit with no resolve**. Abandonment needs no
//! timer, no marker and no sweep: it is the absence of a record.

use serde::{Deserialize, Serialize};

use crate::framing::framed_hex;

/// The hash a chain starts from, so the first entry is framed like every other.
pub const GENESIS: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Body {
    Commit {
        trial: String,
        account: String,
        coordinate: String,
        commitment: String,
        pool_version: u32,
        /// The manifest hash the trial was sealed against, published before the reveal.
        ///
        /// `pool_version` alone is a *pointer*, and a pointer binds nothing: a version can be
        /// re-cut with different images under the same number, and every trial recorded under it
        /// then verifies against whatever manifest the operator happens to serve today. Carrying
        /// the hash in the entry — inside `entry_hash` — is what turns "trust the manifest you are
        /// handed" into "check the manifest you are handed" (D34).
        ///
        /// `None` only on entries written before the field existed. Those predate the binding and
        /// are left exactly as they were: rewriting them to add it would edit an append-only
        /// chain, which is the one thing this record may never do. [`verify`] enforces that the
        /// gap cannot re-open — see [`ChainError::PoolBindingDropped`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pool_manifest_hash: Option<String>,
    },
    Resolve {
        trial: String,
        chosen: String,
        target: String,
        hit: bool,
        /// Both randomness contributions are published, so verification is open to anyone rather
        /// than only to the participant whose browser produced `s_client`.
        s_server: String,
        s_client: String,
        nonce: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub seq: u64,
    pub at: String,
    #[serde(flatten)]
    pub body: Body,
    pub prev: String,
    pub hash: String,
}

impl Body {
    /// Field-framed serialisation. Deliberately not JSON: two implementations must agree on the
    /// bytes, and JSON leaves key order, whitespace and number formatting open to interpretation.
    fn parts(&self) -> Vec<Vec<u8>> {
        match self {
            Body::Commit {
                trial,
                account,
                coordinate,
                commitment,
                pool_version,
                pool_manifest_hash,
            } => {
                let mut parts = vec![
                    b"commit".to_vec(),
                    trial.as_bytes().to_vec(),
                    account.as_bytes().to_vec(),
                    coordinate.as_bytes().to_vec(),
                    commitment.as_bytes().to_vec(),
                    pool_version.to_le_bytes().to_vec(),
                ];
                // Appended rather than written as an empty field when absent, so an entry from
                // before the binding hashes to exactly what it hashed to when it was written.
                // Every field is length-prefixed, so a present-but-empty hash and an absent one
                // are different preimages either way — no entry can be re-read as the other.
                if let Some(hash) = pool_manifest_hash {
                    parts.push(hash.as_bytes().to_vec());
                }
                parts
            }
            Body::Resolve {
                trial,
                chosen,
                target,
                hit,
                s_server,
                s_client,
                nonce,
            } => vec![
                b"resolve".to_vec(),
                trial.as_bytes().to_vec(),
                chosen.as_bytes().to_vec(),
                target.as_bytes().to_vec(),
                vec![*hit as u8],
                s_server.as_bytes().to_vec(),
                s_client.as_bytes().to_vec(),
                nonce.as_bytes().to_vec(),
            ],
        }
    }

    pub fn trial(&self) -> &str {
        match self {
            Body::Commit { trial, .. } | Body::Resolve { trial, .. } => trial,
        }
    }
}

/// `hash = framed(prev, seq, at, …body fields…)`.
pub fn entry_hash(prev: &str, seq: u64, at: &str, body: &Body) -> String {
    let seq_bytes = seq.to_le_bytes();
    let mut parts: Vec<&[u8]> = vec![prev.as_bytes(), &seq_bytes, at.as_bytes()];
    let owned = body.parts();
    for p in &owned {
        parts.push(p);
    }
    framed_hex(&parts)
}

/// Appends in memory. The database transaction that persists an entry is what makes `seq`
/// monotonic in practice; this type owns the hashing rule.
#[derive(Debug, Default)]
pub struct Chain {
    entries: Vec<Entry>,
}

impl Chain {
    pub fn new() -> Self {
        Chain {
            entries: Vec::new(),
        }
    }

    pub fn head(&self) -> &str {
        self.entries
            .last()
            .map(|e| e.hash.as_str())
            .unwrap_or(GENESIS)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn append(&mut self, at: &str, body: Body) -> &Entry {
        let seq = self.entries.len() as u64 + 1;
        let prev = self.head().to_string();
        let hash = entry_hash(&prev, seq, at, &body);
        self.entries.push(Entry {
            seq,
            at: at.to_string(),
            body,
            prev,
            hash,
        });
        self.entries.last().expect("just pushed")
    }

    /// Trials with a commit and no resolve. This is the abandonment rate, computable by anyone
    /// holding the export (FR-027, SC-012).
    pub fn abandoned_trials(&self) -> Vec<&str> {
        let resolved: std::collections::HashSet<&str> = self
            .entries
            .iter()
            .filter(|e| matches!(e.body, Body::Resolve { .. }))
            .map(|e| e.body.trial())
            .collect();
        self.entries
            .iter()
            .filter(|e| matches!(e.body, Body::Commit { .. }))
            .map(|e| e.body.trial())
            .filter(|t| !resolved.contains(t))
            .collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChainError {
    SequenceBroken {
        at: u64,
        expected: u64,
    },
    PrevMismatch {
        seq: u64,
    },
    HashMismatch {
        seq: u64,
    },
    /// A commit with no pool manifest hash, after the log had already begun carrying it.
    ///
    /// Without this rule the optional field of D34 would be a way back out: an operator re-cutting
    /// a pool version could write the trials that follow in the old shape and leave nothing in the
    /// record tying them to any particular manifest. The rule is checkable against the downloaded
    /// file alone — the switch point is wherever the first bound commit sits — so no reader has to
    /// be told from which sequence number to expect it.
    PoolBindingDropped {
        seq: u64,
    },
}

impl std::fmt::Display for ChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainError::SequenceBroken { at, expected } => {
                write!(
                    f,
                    "sequence jumps to {at}, expected {expected} — entries are missing"
                )
            }
            ChainError::PrevMismatch { seq } => {
                write!(f, "entry {seq} does not link to its predecessor")
            }
            ChainError::HashMismatch { seq } => write!(f, "entry {seq} has been altered"),
            ChainError::PoolBindingDropped { seq } => write!(
                f,
                "commit {seq} names no pool manifest hash, but earlier entries do — the binding \
                 cannot be given up once taken"
            ),
        }
    }
}

impl std::error::Error for ChainError {}

/// What a third party runs against the downloaded log. Gaps are as much evidence as alterations:
/// a missing sequence number cannot be explained away.
pub fn verify(entries: &[Entry]) -> Result<(), ChainError> {
    let mut prev = GENESIS.to_string();
    let mut pool_bound = false;
    for (i, e) in entries.iter().enumerate() {
        let expected_seq = i as u64 + 1;
        if e.seq != expected_seq {
            return Err(ChainError::SequenceBroken {
                at: e.seq,
                expected: expected_seq,
            });
        }
        if e.prev != prev {
            return Err(ChainError::PrevMismatch { seq: e.seq });
        }
        if entry_hash(&e.prev, e.seq, &e.at, &e.body) != e.hash {
            return Err(ChainError::HashMismatch { seq: e.seq });
        }
        if let Body::Commit {
            pool_manifest_hash, ..
        } = &e.body
        {
            match pool_manifest_hash {
                Some(_) => pool_bound = true,
                None if pool_bound => return Err(ChainError::PoolBindingDropped { seq: e.seq }),
                None => {}
            }
        }
        prev = e.hash.clone();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(trial: &str) -> Body {
        Body::Commit {
            trial: trial.into(),
            account: "acct".into(),
            coordinate: "4821-9037".into(),
            commitment: "sha256:aa".into(),
            pool_version: 1,
            pool_manifest_hash: Some("sha256:pool".into()),
        }
    }

    /// A commit in the shape written before the pool binding existed.
    fn unbound_commit(trial: &str) -> Body {
        match commit(trial) {
            Body::Commit {
                trial,
                account,
                coordinate,
                commitment,
                pool_version,
                ..
            } => Body::Commit {
                trial,
                account,
                coordinate,
                commitment,
                pool_version,
                pool_manifest_hash: None,
            },
            other => other,
        }
    }

    fn resolve(trial: &str, hit: bool) -> Body {
        Body::Resolve {
            trial: trial.into(),
            chosen: "img_1".into(),
            target: "img_2".into(),
            hit,
            s_server: "aa".into(),
            s_client: "bb".into(),
            nonce: "cc".into(),
        }
    }

    fn sample() -> Chain {
        let mut c = Chain::new();
        c.append("2026-07-25T10:00:00Z", commit("t1"));
        c.append("2026-07-25T10:00:05Z", commit("t2"));
        c.append("2026-07-25T10:00:09Z", resolve("t1", false));
        c
    }

    #[test]
    fn a_fresh_chain_starts_at_genesis() {
        assert_eq!(Chain::new().head(), GENESIS);
    }

    #[test]
    fn a_well_formed_chain_verifies() {
        let c = sample();
        assert_eq!(verify(c.entries()), Ok(()));
        assert_eq!(c.head(), c.entries()[2].hash);
    }

    #[test]
    fn an_abandoned_trial_is_a_commit_without_a_resolve() {
        assert_eq!(sample().abandoned_trials(), vec!["t2"]);
    }

    #[test]
    fn altering_an_entry_is_detected() {
        let c = sample();
        let mut e = c.entries().to_vec();
        e[1].at = "2026-07-25T11:00:00Z".into();
        assert_eq!(verify(&e), Err(ChainError::HashMismatch { seq: 2 }));
    }

    #[test]
    fn removing_an_entry_is_detected() {
        let c = sample();
        let mut e = c.entries().to_vec();
        e.remove(1);
        // The gap shows up as a sequence break before any hash is even checked.
        assert_eq!(
            verify(&e),
            Err(ChainError::SequenceBroken { at: 3, expected: 2 })
        );
    }

    #[test]
    fn relinking_after_a_removal_is_still_detected() {
        // An operator who deletes a trial and renumbers still cannot rebuild the links without
        // the hashes changing, which is the point of carrying `prev` inside the hash.
        let c = sample();
        let mut e = c.entries().to_vec();
        e.remove(1);
        e[1].seq = 2;
        assert_eq!(verify(&e), Err(ChainError::PrevMismatch { seq: 2 }));
    }

    #[test]
    fn the_pool_manifest_hash_is_inside_the_entry_hash() {
        // The point of D34: a pool version re-cut under the same number cannot be slipped past a
        // reader, because the entry that named the old manifest hashes to something else.
        let bound = entry_hash(GENESIS, 1, "t", &commit("t1"));
        assert_ne!(bound, entry_hash(GENESIS, 1, "t", &unbound_commit("t1")));

        let mut other = commit("t1");
        if let Body::Commit {
            pool_manifest_hash, ..
        } = &mut other
        {
            *pool_manifest_hash = Some("sha256:another".into());
        }
        assert_ne!(bound, entry_hash(GENESIS, 1, "t", &other));
    }

    #[test]
    fn an_entry_from_before_the_binding_still_verifies() {
        // Entries written before the field existed are not rewritten to carry it, so a chain that
        // spans the change has to walk end to end exactly as it did.
        let mut c = Chain::new();
        c.append("2026-07-25T10:00:00Z", unbound_commit("t1"));
        c.append("2026-07-26T10:00:00Z", commit("t2"));
        assert_eq!(verify(c.entries()), Ok(()));
    }

    #[test]
    fn the_binding_cannot_be_given_up_once_taken() {
        let mut c = Chain::new();
        c.append("2026-07-26T10:00:00Z", commit("t1"));
        c.append("2026-07-26T10:00:05Z", unbound_commit("t2"));
        assert_eq!(
            verify(c.entries()),
            Err(ChainError::PoolBindingDropped { seq: 2 })
        );
    }

    #[test]
    fn an_entry_from_before_the_binding_serialises_without_the_field() {
        // A field written as `null` would be a third state for verifiers to reason about, and
        // every published copy of the old format would stop matching the file it was taken from.
        let line = serde_json::to_string(&Chain::new().append("t", unbound_commit("t1"))).unwrap();
        assert!(!line.contains("pool_manifest_hash"), "{line}");

        let back: Entry = serde_json::from_str(&line).unwrap();
        assert_eq!(back.body, unbound_commit("t1"));
    }

    #[test]
    fn body_fields_cannot_be_re_split() {
        let a = entry_hash(GENESIS, 1, "t", &commit("ab"));
        let mut other = commit("a");
        if let Body::Commit { account, .. } = &mut other {
            *account = "bacct".into();
        }
        assert_ne!(a, entry_hash(GENESIS, 1, "t", &other));
    }
}
