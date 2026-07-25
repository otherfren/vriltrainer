//! Provenance, licence and attribution, recorded per image (D17).
//!
//! This is the operator's curation interface rather than a build step. Provenance is written at
//! the moment the image is added because a source URL that was not written down then cannot be
//! recovered, and it is the only thing that substantiates the licence — the site runs on a `.de`
//! domain and looks commercial, which makes the operator convenient to send a letter to.
//!
//! None of it reaches the manifest, deliberately. An attribution line rendered beside one image of
//! eight marks that image as reliably as a resolution difference does, which is why the pool is
//! CC0 and public domain only: CC-BY would force exactly that marking (pool-manifest.md, D5).

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Licences whose images may be shown without any visible credit.
///
/// The test is not "is it free" but "can it appear with nothing written next to it". Everything
/// carrying an attribution requirement fails that test regardless of how permissive it otherwise
/// is.
pub const FREE_LICENCES: [&str; 6] = ["CC0", "CC0-1.0", "PD", "PDM", "UNSPLASH", "PEXELS"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Record {
    /// From [`crate::normalise`] — the hash of the normalised bytes.
    pub id: String,
    pub category: String,
    /// What the image is called, in both languages. Optional only so that a catalogue written
    /// before labels existed still loads; `poolctl check` refuses to cut a version without them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<crate::spec::Label>,
    pub source: String,
    pub licence: String,
    /// Kept for the operator's own defence if a licence is ever questioned. Never rendered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    pub added: String,
    /// What the file was called when it was found. Diagnostic only: identity is the hash, so this
    /// can be wrong, duplicated or missing without consequence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
}

/// Everything curated so far, published or not.
///
/// Deliberately separate from any manifest: a pool version is a cut of this at a point in time and
/// is immutable afterwards, while this file keeps growing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalogue {
    #[serde(default)]
    pub images: Vec<Record>,
}

impl Catalogue {
    /// A missing file is an empty catalogue, so a first `add` needs no separate `init`.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Catalogue::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Written sorted by id so the file's diff shows what was curated, not what was reshuffled.
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        self.images.sort_by(|a, b| a.id.cmp(&b.id));
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let mut json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        json.push('\n');
        std::fs::write(path, json).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn get(&self, id: &str) -> Option<&Record> {
        self.images.iter().find(|r| r.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Record> {
        self.images.iter_mut().find(|r| r.id == id)
    }

    /// Refuses a second record for an id. The duplicate is caught here as well as in
    /// [`crate::check`] because the cheapest moment to be told is while the source page is still
    /// open in a browser tab.
    pub fn add(&mut self, record: Record) -> Result<(), String> {
        if let Some(existing) = self.get(&record.id) {
            return Err(format!(
                "already in the pool as {} (added {} from {})",
                existing.id, existing.added, existing.source
            ));
        }
        self.images.push(record);
        Ok(())
    }

    /// Category name to image count, ascending by name.
    pub fn per_category(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for r in &self.images {
            match counts.iter_mut().find(|(c, _)| *c == r.category) {
                Some((_, n)) => *n += 1,
                None => counts.push((r.category.clone(), 1)),
            }
        }
        counts.sort_by(|a, b| a.0.cmp(&b.0));
        counts
    }
}

/// Case- and separator-insensitive, so `CC-0` and `cc0` are not two licences.
pub fn is_free_licence(licence: &str) -> bool {
    let folded: String = licence
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect();
    FREE_LICENCES.iter().any(|l| {
        folded
            == l.chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
    })
}

/// Categories are drawing buckets and are compared by string equality throughout the manifest and
/// the derivation, so `Landscape` and `landscape` would be two buckets holding half a bucket each.
pub fn normalise_category(category: &str) -> String {
    category.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, category: &str) -> Record {
        Record {
            id: id.into(),
            category: category.into(),
            label: Some(crate::spec::Label {
                de: "Name".into(),
                en: "Name".into(),
            }),
            source: "https://example.invalid/x".into(),
            licence: "CC0".into(),
            attribution: None,
            added: "2026-07-25T10:00:00Z".into(),
            original: None,
        }
    }

    #[test]
    fn a_second_copy_of_an_image_is_refused() {
        let mut c = Catalogue::default();
        c.add(record("img_a", "tool")).unwrap();
        assert!(
            c.add(record("img_a", "landscape"))
                .unwrap_err()
                .contains("already in the pool")
        );
    }

    /// The rule that keeps a credit line off the page.
    #[test]
    fn attribution_requiring_licences_are_not_free() {
        for ok in ["CC0", "cc0", "CC0-1.0", "pd", "Unsplash"] {
            assert!(is_free_licence(ok), "{ok} should be accepted");
        }
        for no in ["CC-BY", "CC BY 4.0", "CC-BY-SA", "GFDL", "", "looks free"] {
            assert!(!is_free_licence(no), "{no} should be refused");
        }
    }

    #[test]
    fn catalogue_round_trips_and_sorts() {
        let dir = std::env::temp_dir().join(format!("poolctl-{}", std::process::id()));
        let path = dir.join("catalogue.json");
        let _ = std::fs::remove_file(&path);

        let mut c = Catalogue::load(&path).unwrap();
        assert!(c.images.is_empty(), "a missing catalogue is an empty one");
        c.add(record("img_b", "tool")).unwrap();
        c.add(record("img_a", "landscape")).unwrap();
        c.save(&path).unwrap();

        let back = Catalogue::load(&path).unwrap();
        assert_eq!(
            back.images
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["img_a", "img_b"]
        );
        assert_eq!(
            back.per_category(),
            vec![("landscape".into(), 1), ("tool".into(), 1)]
        );
        std::fs::remove_file(&path).unwrap();
    }
}
