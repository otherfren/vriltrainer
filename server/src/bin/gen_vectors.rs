//! Emits `shared/vectors/derivation.json`, the D7 conformance fixtures.
//!
//! Run deliberately, never as part of a build. Regenerating changes the contract both
//! implementations are held to and invalidates the verifiability of any trial already published.

use server::pool::ImageEntry;
use server::trial::derive::derive;
use server::vectors::{from_hex, members_of, to_hex, Case, Expect};

/// `k` categories with the given sizes; identifiers are zero-padded so manifest order is also
/// ascending lexical order, matching what a real manifest guarantees.
fn build(sizes: &[usize]) -> (Vec<String>, Vec<ImageEntry>) {
    let categories: Vec<String> = (0..sizes.len()).map(|c| format!("cat{c:02}")).collect();
    let mut images = Vec::new();
    let mut n = 0usize;
    for (c, &size) in sizes.iter().enumerate() {
        for _ in 0..size {
            images.push(ImageEntry {
                id: format!("img_{n:05}"),
                category: categories[c].clone(),
            });
            n += 1;
        }
    }
    images.sort_by(|a, b| a.id.cmp(&b.id));
    (categories, images)
}

fn case(name: &str, s_server: &str, s_client: &str, sizes: &[usize]) -> Case {
    let (categories, images) = build(sizes);
    let members = members_of(&categories, &images);
    let d = derive(&from_hex(s_server), &from_hex(s_client), &members)
        .unwrap_or_else(|e| panic!("case {name} does not derive: {e}"));
    Case {
        name: name.to_string(),
        s_server: s_server.to_string(),
        s_client: s_client.to_string(),
        categories,
        images,
        expect: Expect {
            chosen_categories: d.chosen_categories.to_vec(),
            selected_images: d.selected_images.to_vec(),
            target_slot: d.target_slot,
            display_order: d.display_order.to_vec(),
        },
    }
}

fn main() {
    let even20 = vec![20usize; 16];
    let mut lopsided = vec![5usize; 12];
    lopsided[0] = 400;
    let mut singleton = vec![7usize; 10];
    singleton[3] = 1;
    let mut mixed = vec![17usize, 3, 64, 1, 255, 9, 128, 31, 100, 2, 63, 5];
    mixed.extend_from_slice(&[7, 11]);

    let cases = vec![
        // Even categories: the baseline both implementations must agree on.
        case("even-16x20", "00", "00", &even20),
        case("even-16x20-alt-seed", "a1b2c3d4", "0f0e0d0c0b0a", &even20),
        // Exactly eight categories — every one is always chosen, so step 1 degenerates.
        case("minimum-eight-categories", "11223344", "55667788", &[12; 8]),
        // One category holding 400 of 455 images. An implementation that drew the target from the
        // pool rather than from the eight slots would disagree here, not merely differ.
        case("lopsided-400-vs-5", "de", "ad", &lopsided),
        case("lopsided-alt-seed", "beefcafe", "0102030405060708", &lopsided),
        // A category with a single member: `below(1)` must return 0 without consuming forever.
        case("singleton-category", "7f", "80", &singleton),
        // Sizes straddling powers of two, where the rejection bound actually bites.
        case("mixed-sizes-rejection", "0000000000000001", "ffffffffffffffff", &mixed),
    ];

    let json = serde_json::to_string_pretty(&cases).expect("serialise");
    println!("{json}");
    eprintln!("{} cases", cases.len());
    // A visible reminder that the seeds are fixtures, not secrets.
    eprintln!("seed bytes are hex: {}", to_hex(&from_hex("00ff")));
}
