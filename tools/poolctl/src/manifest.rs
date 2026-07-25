//! Building the published manifest (`contracts/pool-manifest.md`).
//!
//! The ordering here is normative, not cosmetic. The derivation indexes into the sorted image list
//! and into the category list, so a manifest that came out in a different order draws different
//! targets from the same seed — silently, and for every future trial. This is why the manifest is
//! generated and never hand-edited.

use serde::{Deserialize, Serialize};
use server::pool::{ImageEntry, Manifest};
use server::trial::derive::SET_SIZE;

use crate::annotate::Record;

/// The published file: the server's [`Manifest`] plus the two fields the contract carries for
/// readers rather than for the derivation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Published {
    pub version: u32,
    pub created: String,
    pub count: usize,
    pub categories: Vec<String>,
    pub images: Vec<ImageEntry>,
    pub manifest_hash: String,
}

impl Published {
    /// The subset the server and the browser verifier actually hash and draw against.
    pub fn manifest(&self) -> Manifest {
        Manifest {
            version: self.version,
            categories: self.categories.clone(),
            images: self.images.clone(),
            manifest_hash: self.manifest_hash.clone(),
        }
    }
}

/// Sort, hash, and refuse anything that would not survive a draw.
///
/// `created` is passed in rather than read from the clock so that building the same cut twice is
/// the same operation everywhere except in that one field.
pub fn build(records: &[Record], version: u32, created: &str) -> Result<Published, String> {
    let mut categories: Vec<String> = records.iter().map(|r| r.category.clone()).collect();
    categories.sort();
    categories.dedup();

    let mut images: Vec<ImageEntry> = records
        .iter()
        .map(|r| ImageEntry { id: r.id.clone(), category: r.category.clone() })
        .collect();
    images.sort_by(|a, b| a.id.cmp(&b.id));

    if categories.len() < SET_SIZE {
        return Err(format!(
            "{} categories, at least {SET_SIZE} are required — a trial shows eight different kinds",
            categories.len()
        ));
    }

    // The category is inside the hash. Hashing identifiers alone would let a category be
    // reassigned without the manifest appearing to change, which alters every future derivation
    // while every published hash still matches (D22).
    let manifest_hash = Manifest::compute_hash(&categories, &images);
    let manifest = Manifest { version, categories, images, manifest_hash };

    // The server's own acceptance rules, run before publication rather than after: sorted, no
    // duplicate ids, no undeclared category, hash agreeing with the pairs.
    manifest.validate()?;

    Ok(Published {
        version,
        created: created.to_string(),
        count: manifest.images.len(),
        categories: manifest.categories,
        images: manifest.images,
        manifest_hash: manifest.manifest_hash,
    })
}

/// Trailing newline, because this file is committed and read by people as well as by programs.
pub fn to_json(published: &Published) -> Result<String, String> {
    let mut json = serde_json::to_string_pretty(published).map_err(|e| e.to_string())?;
    json.push('\n');
    Ok(json)
}
