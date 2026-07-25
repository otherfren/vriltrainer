//! The per-trial commitment, D3.
//!
//! `C = SHA-256( LE64(|s_server|) ‖ s_server ‖ LE64(|nonce|) ‖ nonce ‖ LE64(|coordinate|) ‖ coordinate )`
//!
//! Two properties are load-bearing. The coordinate is **inside** the hash: an earlier draft left
//! it out while claiming the commitment bound it, which meant the same commitment could be paired
//! with any coordinate afterwards. And the fields are **length-prefixed**: plain concatenation is
//! ambiguous across variable-length fields, and relying on fixed formats to save it is an
//! argument that expires the day a format changes.

use crate::framing::framed_hex;

pub fn commitment(s_server: &[u8], nonce: &[u8], coordinate: &str) -> String {
    framed_hex(&[s_server, nonce, coordinate.as_bytes()])
}

/// What the browser does at reveal, and what any third party does from the public log.
pub fn verify(s_server: &[u8], nonce: &[u8], coordinate: &str, claimed: &str) -> bool {
    // Not constant-time on purpose: every input is public by the time it is checked.
    commitment(s_server, nonce, coordinate) == claimed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commitment_is_stable() {
        let c = commitment(b"server", b"nonce", "4821-9037");
        assert_eq!(c, commitment(b"server", b"nonce", "4821-9037"));
        assert!(verify(b"server", b"nonce", "4821-9037", &c));
    }

    /// The property the corrected formula exists for.
    #[test]
    fn a_different_coordinate_breaks_the_commitment() {
        let c = commitment(b"server", b"nonce", "4821-9037");
        assert!(!verify(b"server", b"nonce", "0000-0000", &c));
    }

    /// The property the length prefixes exist for. Under plain concatenation these collided.
    #[test]
    fn field_boundaries_cannot_be_shifted() {
        assert_ne!(commitment(b"ab", b"c", "d"), commitment(b"a", b"bc", "d"));
    }
}
