//! Unambiguous hashing of several fields.
//!
//! Plain concatenation is ambiguous whenever a field's length can vary: `("ab","c")` and
//! `("a","bc")` hash identically. The commitment originally used plain concatenation and was
//! safe only because seeds are 32 bytes and the coordinate is `NNNN-NNNN` — an argument that
//! silently expires the day either format changes. Every field is length-prefixed instead.
//!
//! The same scheme is used for the pool manifest hash, so there is one rule to reason about.

use sha2::{Digest, Sha256};

/// `SHA-256( LE64(len₀) ‖ part₀ ‖ LE64(len₁) ‖ part₁ ‖ … )`
pub fn framed(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_le_bytes());
        h.update(p);
    }
    h.finalize().into()
}

/// The same digest as a `sha256:`-prefixed lowercase hex string, which is how hashes appear in
/// the public log and in the API.
pub fn framed_hex(parts: &[&[u8]]) -> String {
    let d = framed(parts);
    let mut s = String::with_capacity(7 + 64);
    s.push_str("sha256:");
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defect this module exists to remove.
    #[test]
    fn field_boundaries_cannot_be_shifted() {
        assert_ne!(
            framed(&[b"ab", b"c", b"d"]),
            framed(&[b"a", b"bc", b"d"]),
            "length prefixes must make the split unambiguous"
        );
    }

    #[test]
    fn empty_fields_are_distinguishable() {
        assert_ne!(framed(&[b"", b"x"]), framed(&[b"x", b""]));
        assert_ne!(framed(&[b"x"]), framed(&[b"x", b""]));
    }

    #[test]
    fn hex_form_matches_the_digest() {
        let h = framed_hex(&[b"a"]);
        assert!(h.starts_with("sha256:"));
        assert_eq!(h.len(), 7 + 64);
    }
}
