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

/// Panics rather than returns, and names the offending string: the only inputs are the checked-in
/// fixtures, so a failure here is a typo in `derivation.json` and the message is what locates it.
pub fn from_hex(s: &str) -> Vec<u8> {
    hex::decode(s).unwrap_or_else(|e| panic!("{s} is not hex: {e}"))
}

pub fn to_hex(b: &[u8]) -> String {
    hex::encode(b)
}

/// Members per category for a case, through the server's own rule ([`crate::pool`]) and not a copy
/// of it — a fixture built by a second implementation of this ordering would agree with whichever
/// one was wrong.
pub fn members_of(categories: &[String], images: &[ImageEntry]) -> Vec<Vec<usize>> {
    crate::pool::members_by_category(categories, images)
}
