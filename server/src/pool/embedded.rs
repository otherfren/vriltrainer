//! The image bytes, compiled into the binary (D29).
//!
//! `build.rs` generates the table from `pool/normalised/`; this is the only thing that reads it.
//! Nothing here can fail at runtime — the bytes either shipped or they did not, and which of those
//! is true is settled at startup by [`missing`] rather than per request.

include!(concat!(env!("OUT_DIR"), "/pool.rs"));

/// Every image in this build, sorted by id. Public so a test can reach for a real one rather than
/// inventing an identifier; callers want [`get`].
pub fn all() -> &'static [(&'static str, &'static [u8])] {
    IMAGES
}

/// The PNG bytes for an image id, or `None` if this build does not carry it.
pub fn get(id: &str) -> Option<&'static [u8]> {
    // `build.rs` sorts the table, so this is a binary search over ids and not a scan.
    IMAGES
        .binary_search_by_key(&id, |(k, _)| k)
        .ok()
        .map(|i| IMAGES[i].1)
}

/// How many images this build carries. Reported at startup, where a zero is the whole story.
pub fn count() -> usize {
    IMAGES.len()
}

/// The ids a manifest names that this build does not carry.
///
/// The check that makes compiling the pool in worth doing. Serving images from a directory beside
/// the bundle allows a deploy to update the manifest and not the images: the service starts, draws
/// trials, and shows eight broken pictures — a failure that looks like a client bug and is not one.
/// Here the two travel together, so the only way to disagree is to point `--pool` at a manifest
/// from a different build, and that is caught before the first request instead of by a visitor.
pub fn missing<'a>(ids: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    ids.into_iter().filter(|id| get(id).is_none()).collect()
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    /// Every id is the hash of the bytes filed under it.
    ///
    /// The identifier is not a label attached to a picture, it is a claim about the picture's
    /// content — `poolctl` derives it as SHA-256 over exactly these bytes, and the published
    /// manifest repeats it. A curator who runs `sha256sum` on a downloaded image and compares the
    /// first sixteen bytes is performing this test by hand, so it has to hold in the artefact and
    /// not merely at the moment `poolctl import` ran.
    ///
    /// Skipped on a checkout with no images: `pool/normalised/` is generated and gitignored, and a
    /// test that failed on a fresh clone would be a test nobody keeps.
    #[test]
    fn every_identifier_is_the_hash_of_the_bytes_it_names() {
        if count() == 0 {
            return;
        }
        for (id, bytes) in IMAGES {
            let digest = Sha256::digest(bytes);
            let expected: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(
                *id,
                format!("img_{expected}"),
                "an image's bytes are not the ones its identifier claims"
            );
        }
    }

    /// Sorted, because `get` binary-searches. A table that arrived unsorted would answer `None` for
    /// images that are present, and the symptom — some pictures missing, most fine — reads as a
    /// curation problem rather than as this.
    #[test]
    fn the_table_is_sorted_and_every_entry_is_findable() {
        assert!(
            IMAGES.windows(2).all(|w| w[0].0 < w[1].0),
            "the generated table is not sorted by id"
        );
        for (id, bytes) in IMAGES {
            assert_eq!(get(id), Some(*bytes));
        }
        assert_eq!(get("img_0000000000000000000000000000ffff"), None);
    }

    #[test]
    fn missing_names_exactly_what_is_absent() {
        let absent = "img_0000000000000000000000000000ffff";
        assert_eq!(missing([absent]), vec![absent]);
        if let Some((present, _)) = IMAGES.first() {
            assert!(missing([present.to_owned()]).is_empty());
            assert_eq!(missing([present.to_owned(), absent]), vec![absent]);
        }
    }
}
