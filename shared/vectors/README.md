# Derivation — normative specification

This is the contract between the two implementations required by D7: one in Rust on the server,
one in TypeScript in the browser. They must agree byte for byte, or honest trials fail
verification. `derivation.json` in this directory holds the fixtures both must reproduce.

Nothing here may be changed without regenerating the fixtures, and regenerating the fixtures
invalidates every published trial's verifiability. Treat it as frozen.

## Seed

```
seed = SHA-256( s_server ‖ s_client )
```

Raw bytes, concatenated in that order, no separator and no length prefix.

## Keystream

```
block(i) = SHA-256( seed ‖ LE64(i) )     for i = 0, 1, 2, …
```

`LE64(i)` is the 64-bit counter in **little-endian** byte order. Each block yields four 64-bit
words, read **little-endian** from offsets 0, 8, 16, 24. Words are consumed in that order, blocks
in counter order. A draw takes as many words as it needs; nothing is ever pushed back.

## Uniform draw

`below(m)` returns a uniform integer in `[0, m)` by **rejection sampling**:

```
r = 2^64 mod m                      computed as ((2^64 - 1) mod m + 1) mod m
reject w when r != 0 and w >= 2^64 - r
otherwise return w mod m
```

When `m` divides `2^64` — every power of two — nothing is rejected. The bias this removes is on
the order of `10^-17` and would have been harmless; it is removed anyway because the product's
whole claim is that no step has to be taken on trust.

## The four steps

Consumed in this order, from one continuous stream.

### 1 — Eight distinct categories

Partial Fisher-Yates over the category indices `0 … K-1`:

```
c = [0, 1, …, K-1]
for i in 0 … 7:
    j = i + below(K - i)
    swap c[i], c[j]
chosen = c[0 … 7]
```

`chosen` is in **selection order** and is not sorted.

### 2 — One image per chosen category

For each chosen category in selection order:

```
members = manifest images with that category, in manifest order
image[i] = members[ below(len(members)) ]
```

`members` is the sorted manifest **filtered** to that category, preserving manifest order. There
is deliberately no second ordering.

### 3 — The target slot

```
target_slot = below(8)
```

An index into the eight images **in selection order**, before the display shuffle. This step is
what makes the target uniform among the eight shown regardless of how many images each category
holds. Drawing the target from the pool instead would make it proportional to category size, and
the image from the largest category would be the likeliest answer (D22).

### 4 — Display order

Fisher-Yates over eight positions, descending:

```
order = [0, 1, …, 7]
for i from 7 down to 1:
    j = below(i + 1)
    swap order[i], order[j]
```

`order[d]` is the selection index shown at display position `d`. The direction matters: ascending
would consume the stream differently and produce a different permutation from the same seed.

## Fixture format

`derivation.json` is an array of cases:

```jsonc
{
  "name": "uneven-categories",
  "s_server": "<hex>",
  "s_client": "<hex>",
  "categories": ["a", "b", "c"],          // K entries
  "images": [{"id": "img_…", "category": "a"}],   // sorted by id, manifest order
  "expect": {
    "chosen_categories": [4, 1, 7, 0, 5, 2, 6, 3],   // selection order
    "selected_images":   [12, 3, 40, 8, 21, 15, 33, 2],
    "target_slot": 5,
    "display_order": [3, 0, 7, 1, 6, 2, 5, 4]
  }
}
```

Cases deliberately include **uneven category sizes**, so an implementation that drew the target
proportionally to category size would fail rather than merely differ.
