//! The encrypted trial token, D16.
//!
//! Trial working state travels to the client sealed with a key only the server holds, and comes
//! back when the trial resolves. This keeps `s_server` out of the database entirely, so a backup
//! carries no pending answers — the exposure noted in D12.
//!
//! What the token does **not** do is remove the commit row. A stateless server cannot tell it has
//! seen a token before, so the same token could be resubmitted with a different image until it
//! hit. The row written at trial creation is the replay defence, and it is the same row the audit
//! log needs anyway.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};

use crate::framing::framed;

/// Issued with the coordinate. The candidate set does not exist yet — it is derived from
/// `s_client`, which arrives at reveal — so two token states are needed, not one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenOne {
    pub s_server: Vec<u8>,
    pub nonce: Vec<u8>,
    pub coordinate: String,
    pub pool_version: u32,
    /// The manifest the commit entry named, carried so the rest of the trial can be held to it.
    ///
    /// Comparing versions alone is not enough once the version number is known to be re-cuttable
    /// (D34): a trial that starts under one v1 and finishes under another draws its eight images
    /// from a manifest its own commit entry does not describe, and then fails verification for an
    /// honest viewer. Empty only in a token minted before this field existed.
    #[serde(default)]
    pub pool_manifest_hash: String,
}

/// Issued at reveal, carrying everything needed to score the answer without touching the database
/// for anything but the replay check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenTwo {
    pub s_server: Vec<u8>,
    pub s_client: Vec<u8>,
    pub nonce: Vec<u8>,
    pub coordinate: String,
    pub pool_version: u32,
    /// See [`TokenOne::pool_manifest_hash`]. Carried through the reveal, because the answer path
    /// resolves `selected` into image identifiers against whatever pool the process holds now.
    #[serde(default)]
    pub pool_manifest_hash: String,
    /// Manifest indices in selection order.
    pub selected: Vec<usize>,
    pub target_slot: usize,
    pub display_order: Vec<usize>,
    /// Unix seconds. The minimum viewing time is measured from here (FR-039).
    pub revealed_at: i64,
    /// Unix seconds. After this the trial can never resolve (FR-038, D16).
    pub expires_at: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenError {
    Malformed,
    /// Decryption or authentication failed — a wrong key, a tampered token, or a token minted for
    /// a different account or trial.
    NotAuthentic,
    Expired,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::Malformed => write!(f, "token is malformed"),
            TokenError::NotAuthentic => {
                write!(f, "token is not authentic for this account and trial")
            }
            TokenError::Expired => write!(f, "token has expired"),
        }
    }
}

impl std::error::Error for TokenError {}

pub struct Sealer {
    cipher: XChaCha20Poly1305,
}

impl Sealer {
    pub fn new(key: &[u8; 32]) -> Self {
        Sealer {
            cipher: XChaCha20Poly1305::new(Key::from_slice(key)),
        }
    }

    pub fn random_key() -> [u8; 32] {
        use rand::RngCore;
        let mut k = [0u8; 32];
        rand::rng().fill_bytes(&mut k);
        k
    }

    /// Binds the account and the trial sequence as additional authenticated data, so a token
    /// cannot be moved to another account or replayed against another trial. Framed, so the two
    /// fields cannot be re-split.
    fn aad(account_id: &str, seq: u64) -> [u8; 32] {
        framed(&[account_id.as_bytes(), &seq.to_le_bytes()])
    }

    pub fn seal<T: Serialize>(&self, payload: &T, account_id: &str, seq: u64) -> String {
        let plaintext = serde_json::to_vec(payload).expect("payload serialises");
        let aad = Self::aad(account_id, seq);
        // XChaCha20-Poly1305's 192-bit nonce makes random generation safe without a counter,
        // which matters because the server keeps no state between requests (research.md R7).
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ct = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .expect("encryption does not fail with a valid key");
        let mut out = Vec::with_capacity(nonce.len() + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        B64.encode(out)
    }

    pub fn open<T: for<'de> Deserialize<'de>>(
        &self,
        token: &str,
        account_id: &str,
        seq: u64,
    ) -> Result<T, TokenError> {
        let raw = B64.decode(token).map_err(|_| TokenError::Malformed)?;
        if raw.len() < 24 {
            return Err(TokenError::Malformed);
        }
        let (nonce, ct) = raw.split_at(24);
        let aad = Self::aad(account_id, seq);
        let pt = self
            .cipher
            .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad: &aad })
            .map_err(|_| TokenError::NotAuthentic)?;
        serde_json::from_slice(&pt).map_err(|_| TokenError::Malformed)
    }
}

impl TokenTwo {
    /// Whether the trial may still be answered, given the current time in Unix seconds.
    pub fn is_live(&self, now: i64) -> bool {
        now < self.expires_at
    }

    /// Whether enough time has passed since reveal (FR-039).
    ///
    /// The caller must check this **before** looking at the chosen image, or the refusal becomes
    /// an oracle for the target.
    pub fn viewed_long_enough(&self, now: i64, minimum_seconds: i64) -> bool {
        now - self.revealed_at >= minimum_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_two() -> TokenTwo {
        TokenTwo {
            s_server: vec![1; 32],
            s_client: vec![2; 32],
            nonce: vec![3; 32],
            coordinate: "4821-9037".into(),
            pool_version: 3,
            pool_manifest_hash: "sha256:pool".into(),
            selected: vec![10, 20, 30, 40, 50, 60, 70, 80],
            target_slot: 5,
            display_order: vec![3, 0, 7, 1, 6, 2, 5, 4],
            revealed_at: 1_000_000,
            expires_at: 1_086_400,
        }
    }

    #[test]
    fn round_trips() {
        let s = Sealer::new(&Sealer::random_key());
        let t = s.seal(&token_two(), "acct-1", 42);
        let back: TokenTwo = s.open(&t, "acct-1", 42).unwrap();
        assert_eq!(back, token_two());
    }

    #[test]
    fn a_token_cannot_be_moved_to_another_account() {
        let s = Sealer::new(&Sealer::random_key());
        let t = s.seal(&token_two(), "acct-1", 42);
        assert_eq!(
            s.open::<TokenTwo>(&t, "acct-2", 42).unwrap_err(),
            TokenError::NotAuthentic
        );
    }

    #[test]
    fn a_token_cannot_be_replayed_against_another_trial() {
        let s = Sealer::new(&Sealer::random_key());
        let t = s.seal(&token_two(), "acct-1", 42);
        assert_eq!(
            s.open::<TokenTwo>(&t, "acct-1", 43).unwrap_err(),
            TokenError::NotAuthentic
        );
    }

    /// The framing matters here too: without it, ("ab", seq) and ("a", …) could collide.
    #[test]
    fn account_and_sequence_boundaries_are_unambiguous() {
        assert_ne!(Sealer::aad("ab", 1), Sealer::aad("a", 1));
        assert_ne!(Sealer::aad("a", 1), Sealer::aad("a", 2));
    }

    #[test]
    fn tampering_is_detected() {
        let s = Sealer::new(&Sealer::random_key());
        let t = s.seal(&token_two(), "acct-1", 42);
        let mut raw = B64.decode(&t).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 0x01;
        let tampered = B64.encode(raw);
        assert_eq!(
            s.open::<TokenTwo>(&tampered, "acct-1", 42).unwrap_err(),
            TokenError::NotAuthentic
        );
    }

    #[test]
    fn another_key_cannot_open_it() {
        let a = Sealer::new(&Sealer::random_key());
        let b = Sealer::new(&Sealer::random_key());
        let t = a.seal(&token_two(), "acct-1", 42);
        assert_eq!(
            b.open::<TokenTwo>(&t, "acct-1", 42).unwrap_err(),
            TokenError::NotAuthentic
        );
    }

    #[test]
    fn expiry_and_minimum_viewing_time() {
        let t = token_two();
        assert!(t.is_live(1_050_000));
        assert!(!t.is_live(1_086_400));
        assert!(!t.viewed_long_enough(1_000_002, 3));
        assert!(t.viewed_long_enough(1_000_003, 3));
    }
}
