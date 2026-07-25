//! Conformance against `shared/vectors/derivation.json` — the load-bearing test of the project.
//!
//! D7 puts one derivation in Rust and one in TypeScript because a client that verifies the server
//! using the server's own code verifies nothing. These fixtures are what the two agree on. A
//! failure here means honest trials would fail verification in production.

use server::trial::derive::{SET_SIZE, derive};
use server::vectors::{Case, from_hex, members_of};

fn cases() -> Vec<Case> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../shared/vectors/derivation.json"
    );
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {path}: {e} — run `cargo run --bin gen_vectors`"));
    serde_json::from_str(&raw).expect("fixtures are not valid JSON")
}

#[test]
fn every_case_reproduces_exactly() {
    let cases = cases();
    assert!(!cases.is_empty(), "fixture file is empty");
    for c in &cases {
        let members = members_of(&c.categories, &c.images);
        let got = derive(&from_hex(&c.s_server), &from_hex(&c.s_client), &members)
            .unwrap_or_else(|e| panic!("case {}: {e}", c.name));

        assert_eq!(
            got.chosen_categories.to_vec(),
            c.expect.chosen_categories,
            "case {}: categories",
            c.name
        );
        assert_eq!(
            got.selected_images.to_vec(),
            c.expect.selected_images,
            "case {}: images",
            c.name
        );
        assert_eq!(
            got.target_slot, c.expect.target_slot,
            "case {}: target slot",
            c.name
        );
        assert_eq!(
            got.display_order.to_vec(),
            c.expect.display_order,
            "case {}: display order",
            c.name
        );
    }
    eprintln!("{} cases reproduced", cases.len());
}

/// Guards the fixtures themselves. A case with even category sizes cannot detect a
/// size-proportional target draw, so the suite must keep at least one lopsided case.
#[test]
fn fixtures_still_include_a_lopsided_pool() {
    let has_lopsided = cases().iter().any(|c| {
        let m = members_of(&c.categories, &c.images);
        let max = m.iter().map(|v| v.len()).max().unwrap_or(0);
        let min = m.iter().map(|v| v.len()).min().unwrap_or(0);
        max >= min * 10 && min > 0
    });
    assert!(
        has_lopsided,
        "no fixture has wildly uneven categories — the bias D22 exists to prevent would pass unnoticed"
    );
}

/// Fixtures must stay internally consistent: chosen categories distinct, each image drawn from
/// its own category, display order a permutation.
#[test]
fn fixture_expectations_are_self_consistent() {
    for c in cases() {
        let members = members_of(&c.categories, &c.images);

        let mut cats = c.expect.chosen_categories.clone();
        cats.sort_unstable();
        cats.dedup();
        assert_eq!(cats.len(), SET_SIZE, "case {}: categories repeat", c.name);

        for (i, &img) in c.expect.selected_images.iter().enumerate() {
            let cat = c.expect.chosen_categories[i];
            assert!(
                members[cat].contains(&img),
                "case {}: image {i} is not in its category",
                c.name
            );
        }

        let mut order = c.expect.display_order.clone();
        order.sort_unstable();
        assert_eq!(
            order,
            (0..SET_SIZE).collect::<Vec<_>>(),
            "case {}: display order",
            c.name
        );
        assert!(
            c.expect.target_slot < SET_SIZE,
            "case {}: target slot out of range",
            c.name
        );
    }
}
