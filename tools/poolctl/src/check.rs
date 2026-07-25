//! The gate that runs while a fault is still cheap to fix (T093).
//!
//! Everything here is checkable before a version is cut, and nothing here is checkable afterwards:
//! a pool version is immutable, so an image that entered it without a source stays in it, and a
//! category that was too thin repeats in front of players for as long as that version is live.

use std::path::Path;

use server::trial::derive::SET_SIZE;

use crate::annotate::{Catalogue, is_free_licence};

/// Images at launch (D5, amended). Below this a player meets the same image often enough for the
/// pool to look like a demo.
pub const LAUNCH_FLOOR: usize = 160;

/// Categories per pool (D22). Fewer than [`SET_SIZE`] cannot fill a trial at all; the band is what
/// keeps a set from looking like the same eight kinds of thing every round.
pub const CATEGORIES_MIN: usize = 16;
pub const CATEGORIES_MAX: usize = 24;

/// Images below which a category is called thin.
///
/// A category appears in 8 of every K trials, and within it images are drawn uniformly, so a
/// category of `n` images shows a repeat after roughly `sqrt(n)` of its own appearances. At n = 6
/// that is under three appearances — visible as repetition long before the pool as a whole runs
/// short. The point of naming it here is that it becomes visible *before* it starts repeating.
pub const THIN_CATEGORY: usize = 12;

#[derive(Debug, Default)]
pub struct Report {
    /// Faults that make the pool wrong. A build must not proceed over these.
    pub errors: Vec<String>,
    /// Faults that make the pool poor. Worth seeing every time, worth blocking nothing.
    pub warnings: Vec<String>,
    pub per_category: Vec<(String, usize)>,
    pub total: usize,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// `images_dir` is where [`crate::normalise`] wrote the published bytes; a record whose file is
/// gone would build a manifest entry nobody can display.
pub fn check(catalogue: &Catalogue, images_dir: &Path) -> Report {
    let mut r = Report { total: catalogue.images.len(), ..Report::default() };

    let mut seen: Vec<&str> = Vec::new();
    for rec in &catalogue.images {
        // Provenance is the whole defence if a licence is ever questioned, and it cannot be
        // reconstructed later — the source page moves, the file stays.
        if rec.source.trim().is_empty() {
            r.errors.push(format!("{}: no source", rec.id));
        }
        if rec.licence.trim().is_empty() {
            r.errors.push(format!("{}: no licence", rec.id));
        } else if !is_free_licence(&rec.licence) {
            r.errors.push(format!(
                "{}: licence {} requires visible attribution, which would mark it among the eight",
                rec.id, rec.licence
            ));
        }
        if rec.category.trim().is_empty() {
            r.errors.push(format!("{}: no category", rec.id));
        }

        // Identity is the hash of the normalised bytes, so a duplicate id is the same image found
        // twice — under a different filename, from a different source page, or simply forgotten.
        if seen.contains(&rec.id.as_str()) {
            r.errors.push(format!("{}: duplicate of an image already in the pool", rec.id));
        } else {
            seen.push(&rec.id);
        }

        let file = images_dir.join(format!("{}.png", rec.id));
        if !file.exists() {
            r.errors.push(format!("{}: normalised file {} is missing", rec.id, file.display()));
        }
    }

    r.per_category = catalogue.per_category();
    let k = r.per_category.len();

    if k < SET_SIZE {
        r.errors.push(format!(
            "{k} categories, at least {SET_SIZE} are required — a trial shows eight different kinds"
        ));
    } else if k < CATEGORIES_MIN {
        r.warnings.push(format!("{k} categories, {CATEGORIES_MIN} to {CATEGORIES_MAX} intended"));
    } else if k > CATEGORIES_MAX {
        r.warnings.push(format!(
            "{k} categories, more than the {CATEGORIES_MAX} intended — cut finer only while each \
             stays stocked"
        ));
    }

    for (name, n) in &r.per_category {
        if *n < THIN_CATEGORY {
            r.warnings.push(format!(
                "category {name} holds {n} images, under {THIN_CATEGORY} — it will repeat visibly"
            ));
        }
    }

    if r.total < LAUNCH_FLOOR {
        r.warnings.push(format!("{} images, {LAUNCH_FLOOR} intended at launch", r.total));
    }

    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::Record;

    fn record(id: &str, category: &str) -> Record {
        Record {
            id: id.into(),
            category: category.into(),
            source: "https://example.invalid/x".into(),
            licence: "CC0".into(),
            attribution: None,
            added: "2026-07-25T10:00:00Z".into(),
            original: None,
        }
    }

    /// A directory containing a file per record, so the file-existence rule does not drown the
    /// assertion under test.
    fn staged(records: &[Record]) -> (std::path::PathBuf, Catalogue) {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("poolctl-check-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut c = Catalogue::default();
        for r in records {
            std::fs::write(dir.join(format!("{}.png", r.id)), b"x").unwrap();
            c.images.push(r.clone());
        }
        (dir, c)
    }

    fn full_pool() -> Vec<Record> {
        (0..16)
            .flat_map(|c| {
                (0..12).map(move |i| record(&format!("img_{c:02}{i:02}"), &format!("cat{c:02}")))
            })
            .collect()
    }

    #[test]
    fn a_stocked_pool_passes_clean() {
        let (dir, c) = staged(&full_pool());
        let r = check(&c, &dir);
        assert!(r.ok(), "{:?}", r.errors);
        assert_eq!(r.per_category.len(), 16);
        assert_eq!(r.total, 192);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn missing_provenance_is_an_error_not_a_warning() {
        let mut records = full_pool();
        records[0].source = String::new();
        records[1].licence = String::new();
        records[2].category = String::new();
        records[3].licence = "CC-BY-SA".into();
        let (dir, c) = staged(&records);
        let r = check(&c, &dir);
        assert!(r.errors.iter().any(|e| e.contains("no source")));
        assert!(r.errors.iter().any(|e| e.contains("no licence")));
        assert!(r.errors.iter().any(|e| e.contains("no category")));
        assert!(r.errors.iter().any(|e| e.contains("visible attribution")));
    }

    #[test]
    fn the_same_image_twice_is_an_error() {
        let mut records = full_pool();
        let mut again = records[0].clone();
        again.category = "cat05".into();
        records.push(again);
        let (dir, c) = staged(&records);
        assert!(check(&c, &dir).errors.iter().any(|e| e.contains("duplicate")));
    }

    /// The reason this check exists: a thin category has to be visible before players meet it.
    #[test]
    fn a_thin_category_is_reported_before_it_repeats() {
        let mut records = full_pool();
        records.retain(|r| r.category != "cat03" || r.id.ends_with("00"));
        let (dir, c) = staged(&records);
        let r = check(&c, &dir);
        assert!(r.ok(), "a thin category is poor curation, not a broken pool");
        assert!(r.warnings.iter().any(|w| w.contains("category cat03 holds 1 images")));
        assert_eq!(r.per_category.iter().find(|(n, _)| n == "cat03").unwrap().1, 1);
    }

    #[test]
    fn too_few_categories_cannot_fill_a_trial() {
        let records: Vec<Record> = (0..4)
            .flat_map(|c| {
                (0..12).map(move |i| record(&format!("img_{c:02}{i:02}"), &format!("cat{c:02}")))
            })
            .collect();
        let (dir, c) = staged(&records);
        let r = check(&c, &dir);
        assert!(!r.ok());
        assert!(r.errors.iter().any(|e| e.contains("at least 8 are required")));
        assert!(r.warnings.iter().any(|w| w.contains("intended at launch")));
    }

    #[test]
    fn a_record_without_its_file_is_an_error() {
        let (dir, mut c) = staged(&full_pool());
        c.images.push(record("img_ghost", "cat00"));
        assert!(check(&c, &dir).errors.iter().any(|e| e.contains("is missing")));
    }
}
