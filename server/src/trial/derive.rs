//! Trial derivation — the four steps of D22, specified in `shared/vectors/README.md`.
//!
//! This is one of two implementations. The other lives in the browser, in TypeScript, and both
//! must agree on `shared/vectors/derivation.json`. Nothing here may drift without regenerating
//! those fixtures, which would invalidate the verifiability of every published trial.

use sha2::{Digest, Sha256};

/// Images shown per trial (D8).
pub const SET_SIZE: usize = 8;

/// The keystream: `SHA-256(seed ‖ LE64(counter))`, consumed as little-endian 64-bit words.
pub struct Stream {
    seed: [u8; 32],
    block: [u8; 32],
    counter: u64,
    offset: usize,
}

impl Stream {
    /// `seed = framed(s_server, s_client)` — length-prefixed for the same reason the commitment
    /// is. Both contributions are 32 bytes today; the framing means that stops mattering.
    pub fn new(s_server: &[u8], s_client: &[u8]) -> Self {
        Stream {
            seed: crate::framing::framed(&[s_server, s_client]),
            block: [0u8; 32],
            counter: 0,
            // Forces a block to be produced on the first word.
            offset: 32,
        }
    }

    fn next_word(&mut self) -> u64 {
        if self.offset >= 32 {
            let mut h = Sha256::new();
            h.update(self.seed);
            h.update(self.counter.to_le_bytes());
            self.block = h.finalize().into();
            self.counter += 1;
            self.offset = 0;
        }
        let w = u64::from_le_bytes(self.block[self.offset..self.offset + 8].try_into().unwrap());
        self.offset += 8;
        w
    }

    /// Uniform integer in `[0, m)` by rejection sampling.
    ///
    /// The rejection bound is `floor(2^64 / m) * m`, which equals `2^64 - (2^64 mod m)`. Neither
    /// fits in a `u64`, so both are expressed relative to `u64::MAX`. When `m` divides `2^64`
    /// nothing is rejected.
    pub fn below(&mut self, m: u64) -> u64 {
        assert!(m > 0, "below(0) is undefined");
        let two64_mod_m = ((u64::MAX % m) + 1) % m;
        loop {
            let w = self.next_word();
            if two64_mod_m == 0 || w < u64::MAX - two64_mod_m + 1 {
                return w % m;
            }
        }
    }
}

/// What one trial's derivation produces. All indices are into the caller's manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draw {
    /// Category indices, in selection order, never sorted.
    pub chosen_categories: [usize; SET_SIZE],
    /// Manifest image indices, in selection order, one per chosen category.
    pub selected_images: [usize; SET_SIZE],
    /// Which of the eight is the target, in *selection* order.
    pub target_slot: usize,
    /// `display_order[d]` is the selection index shown at display position `d`.
    pub display_order: [usize; SET_SIZE],
}

impl Draw {
    /// The manifest index of the target image.
    pub fn target_image(&self) -> usize {
        self.selected_images[self.target_slot]
    }

    /// Manifest indices in the order they are shown to the viewer.
    pub fn images_in_display_order(&self) -> [usize; SET_SIZE] {
        let mut out = [0usize; SET_SIZE];
        for d in 0..SET_SIZE {
            out[d] = self.selected_images[self.display_order[d]];
        }
        out
    }
}

/// Errors that make a pool unusable for a draw. All of them are curation faults, not runtime
/// conditions, so they are worth failing loudly rather than degrading.
#[derive(Debug, PartialEq, Eq)]
pub enum DeriveError {
    /// Fewer than eight categories: a trial could not show eight distinct kinds.
    TooFewCategories(usize),
    /// A category holds no images, so it can never contribute one.
    EmptyCategory(usize),
}

impl std::fmt::Display for DeriveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeriveError::TooFewCategories(k) => {
                write!(f, "pool has {k} categories, at least {SET_SIZE} are required")
            }
            DeriveError::EmptyCategory(c) => write!(f, "category index {c} holds no images"),
        }
    }
}

impl std::error::Error for DeriveError {}

/// Run the four steps.
///
/// `members[c]` is the manifest indices of category `c`, in manifest order — the sorted manifest
/// filtered to that category, which is the only ordering that exists.
pub fn derive(
    s_server: &[u8],
    s_client: &[u8],
    members: &[Vec<usize>],
) -> Result<Draw, DeriveError> {
    let k = members.len();
    if k < SET_SIZE {
        return Err(DeriveError::TooFewCategories(k));
    }
    if let Some(c) = members.iter().position(|m| m.is_empty()) {
        return Err(DeriveError::EmptyCategory(c));
    }

    let mut st = Stream::new(s_server, s_client);

    // 1 — eight distinct categories, partial Fisher-Yates.
    let mut cats: Vec<usize> = (0..k).collect();
    for i in 0..SET_SIZE {
        let j = i + st.below((k - i) as u64) as usize;
        cats.swap(i, j);
    }
    let mut chosen_categories = [0usize; SET_SIZE];
    chosen_categories.copy_from_slice(&cats[..SET_SIZE]);

    // 2 — one image from each, in selection order.
    let mut selected_images = [0usize; SET_SIZE];
    for i in 0..SET_SIZE {
        let m = &members[chosen_categories[i]];
        selected_images[i] = m[st.below(m.len() as u64) as usize];
    }

    // 3 — the target slot. Uniform over the eight shown, independent of category sizes.
    let target_slot = st.below(SET_SIZE as u64) as usize;

    // 4 — display order, descending Fisher-Yates.
    let mut display_order = [0usize; SET_SIZE];
    for (i, slot) in display_order.iter_mut().enumerate() {
        *slot = i;
    }
    for i in (1..SET_SIZE).rev() {
        let j = st.below((i + 1) as u64) as usize;
        display_order.swap(i, j);
    }

    Ok(Draw {
        chosen_categories,
        selected_images,
        target_slot,
        display_order,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn even_pool(k: usize, per: usize) -> Vec<Vec<usize>> {
        (0..k)
            .map(|c| (0..per).map(|i| c * per + i).collect())
            .collect()
    }

    #[test]
    fn same_inputs_give_the_same_draw() {
        let p = even_pool(20, 25);
        let a = derive(b"server-seed", b"client-seed", &p).unwrap();
        let b = derive(b"server-seed", b"client-seed", &p).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn categories_are_distinct_and_images_come_from_them() {
        let p = even_pool(20, 25);
        let d = derive(b"s", b"c", &p).unwrap();
        let mut seen = d.chosen_categories;
        seen.sort_unstable();
        seen.windows(2).for_each(|w| assert_ne!(w[0], w[1]));
        for i in 0..SET_SIZE {
            assert!(p[d.chosen_categories[i]].contains(&d.selected_images[i]));
        }
    }

    #[test]
    fn display_order_is_a_permutation() {
        let d = derive(b"s", b"c", &even_pool(12, 4)).unwrap();
        let mut o = d.display_order;
        o.sort_unstable();
        assert_eq!(o, [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    /// The property D22 exists for. With wildly uneven categories, the target must still land on
    /// each slot equally often — a size-proportional draw would fail this badly.
    #[test]
    fn target_slot_is_uniform_regardless_of_category_sizes() {
        let mut members: Vec<Vec<usize>> = Vec::new();
        let mut next = 0usize;
        for c in 0..10 {
            let size = if c == 0 { 400 } else { 5 };
            members.push((0..size).map(|_| { next += 1; next }).collect());
        }
        let mut counts = [0u32; SET_SIZE];
        for i in 0..40_000u32 {
            let d = derive(&i.to_le_bytes(), b"fixed", &members).unwrap();
            counts[d.target_slot] += 1;
        }
        let expected = 40_000.0 / SET_SIZE as f64;
        for (slot, &n) in counts.iter().enumerate() {
            let dev = (n as f64 - expected).abs() / expected;
            assert!(dev < 0.05, "slot {slot} off by {:.1}%: {n}", dev * 100.0);
        }
    }

    /// Always choosing the image drawn from the largest category must score chance, not better.
    #[test]
    fn largest_category_strategy_scores_chance() {
        let mut members: Vec<Vec<usize>> = vec![(0..400).collect()];
        let mut next = 400usize;
        for _ in 1..10 {
            members.push((0..5).map(|_| { next += 1; next }).collect());
        }
        let mut hits = 0u32;
        let rounds = 40_000u32;
        for i in 0..rounds {
            let d = derive(&i.to_le_bytes(), b"fixed", &members).unwrap();
            // Category 0 is the big one; bet on it whenever it was drawn.
            if let Some(slot) = d.chosen_categories.iter().position(|&c| c == 0) {
                if slot == d.target_slot {
                    hits += 1;
                }
            }
        }
        // Category 0 appears in 8 of 10 draws; when it does, it is the target 1 time in 8.
        let expected = rounds as f64 * 0.8 / 8.0;
        let dev = (hits as f64 - expected).abs() / expected;
        assert!(dev < 0.06, "expected ≈{expected:.0}, got {hits}");
    }

    #[test]
    fn rejects_pools_that_cannot_fill_a_trial() {
        assert_eq!(
            derive(b"s", b"c", &even_pool(7, 10)),
            Err(DeriveError::TooFewCategories(7))
        );
        let mut p = even_pool(10, 3);
        p[4].clear();
        assert_eq!(derive(b"s", b"c", &p), Err(DeriveError::EmptyCategory(4)));
    }
}
