//! Shape of `shared/vectors/derivation.json` — the D7 contract between the two implementations.

use crate::pool::ImageEntry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Expect {
    pub chosen_categories: Vec<usize>,
    pub selected_images: Vec<usize>,
    pub target_slot: usize,
    pub display_order: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Case {
    pub name: String,
    /// Hex, so the fixtures stay readable and language-neutral.
    pub s_server: String,
    pub s_client: String,
    pub categories: Vec<String>,
    pub images: Vec<ImageEntry>,
    pub expect: Expect,
}

pub fn from_hex(s: &str) -> Vec<u8> {
    assert!(s.len() % 2 == 0, "hex string has odd length: {s}");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("invalid hex"))
        .collect()
}

pub fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Members per category, from a case's raw category and image lists.
pub fn members_of(categories: &[String], images: &[ImageEntry]) -> Vec<Vec<usize>> {
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
