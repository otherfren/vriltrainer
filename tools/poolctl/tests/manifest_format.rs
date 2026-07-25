//! The manifest's ordering and hash are a contract, not an implementation detail (T084).
//!
//! The derivation indexes into the sorted image list, so a manifest that came out in a different
//! order draws a different target from the same seed. That failure is silent — every trial still
//! verifies against its own recomputation — which is why it is asserted here rather than trusted.

use poolctl::annotate::Record;
use poolctl::manifest::{self, Published};
use server::pool::Manifest;
use server::trial::derive::derive;

fn record(id: &str, category: &str) -> Record {
    Record {
        id: id.into(),
        category: category.into(),
        label: Some(poolctl::spec::Label {
            de: "Name".into(),
            en: "Name".into(),
        }),
        source: "https://example.invalid/page".into(),
        licence: "CC0".into(),
        attribution: None,
        added: "2026-07-25T10:00:00Z".into(),
        original: None,
    }
}

/// Deliberately in no useful order, and with uneven categories: the build has to do the sorting,
/// and a pool in the wild is never balanced.
fn records() -> Vec<Record> {
    let sizes = [7usize, 3, 11, 2, 5, 9, 4, 6, 8, 1, 12, 3];
    let mut out = Vec::new();
    for (c, n) in sizes.iter().enumerate() {
        for i in 0..*n {
            out.push(record(
                &format!("img_{:04x}", (c * 97 + i * 13) as u32),
                &format!("cat{c:02}"),
            ));
        }
    }
    out.reverse();
    out
}

fn built() -> Published {
    manifest::build(&records(), 1, "2026-07-25T10:00:00Z").expect("the fixture pool is buildable")
}

#[test]
fn images_are_sorted_ascending_by_id() {
    let p = built();
    assert!(
        p.images.windows(2).all(|w| w[0].id < w[1].id),
        "the input order must not survive into the manifest"
    );
    assert_eq!(p.count, p.images.len());
    // Categories are indexed by the derivation too, so their order is fixed as well.
    assert!(p.categories.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn the_hash_covers_the_sorted_list() {
    let p = built();
    assert_eq!(
        p.manifest_hash,
        Manifest::compute_hash(&p.categories, &p.images)
    );
    assert!(p.manifest_hash.starts_with("sha256:"));
    assert!(p.manifest().hash_matches());
}

/// The whole reason the ordering is normative.
#[test]
fn a_reordered_manifest_is_rejected() {
    let p = built();

    let mut swapped = p.manifest();
    swapped.images.swap(0, 1);
    // Without a recomputed hash, the reordering shows up as a hash mismatch.
    assert!(swapped.validate().is_err());

    // With one, the ordering rule has to catch it on its own — an operator who re-sorts and
    // rebuilds produces a self-consistent manifest that draws different targets.
    swapped.manifest_hash = Manifest::compute_hash(&swapped.categories, &swapped.images);
    assert!(swapped.hash_matches());
    let err = swapped
        .validate()
        .expect_err("a self-consistent but unsorted manifest is still wrong");
    assert!(err.contains("sorted"), "{err}");
}

/// What the reordering would have cost, stated as the outcome rather than as a rule.
#[test]
fn reordering_changes_what_a_seed_draws() {
    let p = built();
    let honest = p.manifest();

    let mut reversed = honest.clone();
    reversed.images.reverse();
    reversed.manifest_hash = Manifest::compute_hash(&reversed.categories, &reversed.images);

    let a = derive(b"s_server", b"s_client", &honest.members()).unwrap();
    let b = derive(b"s_server", b"s_client", &reversed.members()).unwrap();
    assert_ne!(
        a.selected_images, b.selected_images,
        "if this ever holds, the ordering is not load-bearing and this test is lying"
    );
}

/// The published file is read back by the server and by the browser verifier. A field renamed on
/// either side breaks verification for every trial under that version.
#[test]
fn the_published_json_is_what_the_server_reads() {
    let json = manifest::to_json(&built()).unwrap();
    let parsed: Manifest = serde_json::from_str(&json).expect("the server's type must accept it");
    assert_eq!(parsed.validate(), Ok(()));
    assert_eq!(parsed, built().manifest());

    // `created` and `count` are for readers; the server ignores them and must not choke on them.
    let raw: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(raw["created"].is_string());
    assert_eq!(raw["count"].as_u64().unwrap() as usize, parsed.images.len());
}

#[test]
fn a_pool_that_cannot_fill_a_trial_is_not_published() {
    let thin: Vec<Record> = (0..7)
        .map(|c| record(&format!("img_{c:04}"), &format!("cat{c:02}")))
        .collect();
    let err = manifest::build(&thin, 1, "2026-07-25T10:00:00Z").expect_err("7 categories is not 8");
    assert!(err.contains("at least 8"), "{err}");
}
