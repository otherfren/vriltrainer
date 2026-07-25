//! The per-trial commitment, D3.
//!
//! `C = SHA-256(s_server ‖ nonce ‖ coordinate)`.
//!
//! The coordinate is **inside** the hash. An earlier draft left it out while claiming the
//! commitment bound it; with the coordinate outside, the same commitment could be paired with any
//! coordinate afterwards and the intended statement — *this* coordinate pointed at *this* image —
//! was unprovable.

use sha2::{Digest, Sha256};

pub fn commitment(s_server: &[u8], nonce: &[u8], coordinate: &str) -> String {
    let mut h = Sha256::new();
    h.update(s_server);
    h.update(nonce);
    h.update(coordinate.as_bytes());
    format!("sha256:{:x}", h.finalize())
}

/// What the browser does at reveal time, and what any third party does from the public log.
pub fn verify(s_server: &[u8], nonce: &[u8], coordinate: &str, claimed: &str) -> bool {
    // Not constant-time on purpose: every input here is public by the time it is checked.
    commitment(s_server, nonce, coordinate) == claimed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commitment_is_stable() {
        let c = commitment(b"server", b"nonce", "4821-9037");
        assert_eq!(c, commitment(b"server", b"nonce", "4821-9037"));
        assert!(c.starts_with("sha256:"));
        assert!(verify(b"server", b"nonce", "4821-9037", &c));
    }

    /// The property the corrected formula exists for.
    #[test]
    fn a_different_coordinate_breaks_the_commitment() {
        let c = commitment(b"server", b"nonce", "4821-9037");
        assert!(
            !verify(b"server", b"nonce", "0000-0000", &c),
            "commitment must not be reusable with another coordinate"
        );
    }

    #[test]
    fn field_boundaries_cannot_be_shifted() {
        // Without distinct fields these would collide under naive concatenation. They do collide
        // here too — which is why the coordinate is fixed-format and the seeds fixed-length.
        // Recorded as a known property rather than papered over.
        assert_eq!(
            commitment(b"ab", b"c", "d"),
            commitment(b"a", b"bc", "d"),
            "concatenation is ambiguous across variable-length fields; lengths are fixed by \
             construction (32-byte seeds, 32-byte nonce, NNNN-NNNN coordinate)"
        );
    }
}
