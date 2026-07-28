//! The published image manifest, per `specs/001-remote-viewing-trainer/contracts/pool-manifest.md`.

pub mod embedded;
pub mod record;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageEntry {
    pub id: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub version: u32,
    pub categories: Vec<String>,
    /// Sorted ascending by `id`. **This order is the index the derivation draws against.**
    pub images: Vec<ImageEntry>,
    pub manifest_hash: String,
}

/// Manifest indices per category, in manifest order — the input the derivation draws against.
///
/// A category's member list is the sorted manifest *filtered* to that category. There is
/// deliberately no second ordering to keep in sync — see the contract.
///
/// Free-standing rather than a method because the test vectors are built from loose category and
/// image lists that never become a [`Manifest`] ([`crate::vectors::members_of`]). A second copy of
/// this loop there would let the fixtures and the server disagree about which image sits at which
/// index, which is precisely the divergence the vectors exist to catch — and they would confirm
/// each other instead.
pub fn members_by_category(categories: &[String], images: &[ImageEntry]) -> Vec<Vec<usize>> {
    categories
        .iter()
        .map(|c| {
            images
                .iter()
                .enumerate()
                .filter(|(_, e)| &e.category == c)
                .map(|(i, _)| i)
                .collect()
        })
        .collect()
}

impl Manifest {
    /// See [`members_by_category`].
    pub fn members(&self) -> Vec<Vec<usize>> {
        members_by_category(&self.categories, &self.images)
    }

    /// Hash over the sorted `(id, category)` pairs.
    ///
    /// The category is inside the hash on purpose. Hashing identifiers alone would let a category
    /// be reassigned without the manifest appearing to change, silently altering every future
    /// derivation (D22).
    pub fn compute_hash(categories: &[String], images: &[ImageEntry]) -> String {
        let mut h = Sha256::new();
        h.update((categories.len() as u64).to_le_bytes());
        for c in categories {
            h.update((c.len() as u64).to_le_bytes());
            h.update(c.as_bytes());
        }
        h.update((images.len() as u64).to_le_bytes());
        for e in images {
            h.update((e.id.len() as u64).to_le_bytes());
            h.update(e.id.as_bytes());
            h.update((e.category.len() as u64).to_le_bytes());
            h.update(e.category.as_bytes());
        }
        format!("sha256:{:x}", h.finalize())
    }

    /// Rebuild the hash and compare. Cheap, and the only thing standing between a hand-edited
    /// manifest and silently different trials.
    pub fn hash_matches(&self) -> bool {
        Self::compute_hash(&self.categories, &self.images) == self.manifest_hash
    }

    /// Sorted-by-id, no duplicate identifiers, every image in a declared category.
    pub fn validate(&self) -> Result<(), String> {
        if !self.images.windows(2).all(|w| w[0].id < w[1].id) {
            return Err("images are not sorted ascending by id, or contain duplicates".into());
        }
        for e in &self.images {
            if !self.categories.contains(&e.category) {
                return Err(format!(
                    "image {} has undeclared category {}",
                    e.id, e.category
                ));
            }
        }
        if !self.hash_matches() {
            return Err("manifest_hash does not match the (id, category) pairs".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, cat: &str) -> ImageEntry {
        ImageEntry {
            id: id.into(),
            category: cat.into(),
        }
    }

    fn manifest() -> Manifest {
        let categories = vec!["a".to_string(), "b".to_string()];
        let images = vec![
            entry("img_1", "a"),
            entry("img_2", "b"),
            entry("img_3", "a"),
        ];
        let manifest_hash = Manifest::compute_hash(&categories, &images);
        Manifest {
            version: 1,
            categories,
            images,
            manifest_hash,
        }
    }

    #[test]
    fn members_filter_in_manifest_order() {
        assert_eq!(manifest().members(), vec![vec![0, 2], vec![1]]);
    }

    #[test]
    fn reassigning_a_category_changes_the_hash() {
        let m = manifest();
        let mut moved = m.images.clone();
        moved[0].category = "b".into();
        assert_ne!(
            Manifest::compute_hash(&m.categories, &m.images),
            Manifest::compute_hash(&m.categories, &moved),
            "a category move must not be invisible to the hash"
        );
    }

    #[test]
    fn validate_catches_unsorted_and_undeclared() {
        let mut m = manifest();
        m.images.swap(0, 1);
        assert!(m.validate().is_err());

        let mut m = manifest();
        m.images.push(entry("img_9", "zzz"));
        m.manifest_hash = Manifest::compute_hash(&m.categories, &m.images);
        assert!(m.validate().unwrap_err().contains("undeclared category"));
    }
}
