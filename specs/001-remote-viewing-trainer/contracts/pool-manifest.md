# Contract — Pool manifest format

Published per pool version. Anyone recomputing a trial needs it, so it is a contract rather than
an internal asset.

```jsonc
{
  "version": 3,
  "created": "2026-07-20T09:14:00Z",
  "count": 512,
  "categories": ["architecture", "landscape", "tool", "…"],   // K entries, 16-24
  "images": [                                            // ascending by id, this order is the index
    { "id": "img_04c1…", "category": "tool" },
    { "id": "img_09fe…", "category": "landscape" }
  ],
  "manifest_hash": "sha256:…"                            // over the categories and the sorted pairs
}
```

## Rules

- **`images` is sorted ascending by id and its order *is* the index** the derivation draws
  against (R1). A different order produces different targets from the same seed, so the ordering
  is normative, not cosmetic.
- **A category's member list is the sorted list filtered to that category**, preserving order.
  There is deliberately no second ordering to keep in sync.
- **`manifest_hash` covers the category list and the sorted (id, category) pairs, not the ids
  alone.** The preimage is
  `LE64(K) ‖ (LE64(|c|) ‖ c for each category, in manifest order) ‖ LE64(N) ‖ (LE64(|id|) ‖ id ‖ LE64(|category|) ‖ category for each image, in manifest order)`,
  hashed with SHA-256 and written as `sha256:<lowercase hex>`. The two collection lengths are bare
  counts, so this is deliberately *not* `framed()` over the pairs. Hashing only the ids would let
  a category be reassigned without changing the hash, silently altering every future derivation
  while the manifest appeared untouched (D22).
- A Merkle root was specified originally and dropped: a tree buys inclusion proofs without a full
  download, and the manifest is published whole (D5, D17).
- **`img_…` identifiers are hashes of the normalised image bytes.** Identity follows content, so
  a filename or a re-upload cannot change what an identifier means.
- **A new version is created for every change**, never an edit in place. Trials reference the
  version they ran under and stay verifiable forever (FR-012).

## Normalisation, and why the manifest is silent about provenance

Every image passes the same pipeline before its identifier exists: fixed edge length, uniform
requantisation, metadata stripped. Anything that could distinguish the target from its seven
decoys — resolution, aspect ratio, compression artefacts, the colour signature of one source —
is a sensory channel, and sensory leakage is the classic way forced-choice ESP experiments
produce false positives (D5).

Provenance, licence and attribution are tracked per image by the curation tool, but they are
**not** in the manifest and are never rendered beside a candidate. Attribution shown next to one
image of eight would mark it. This is why the trial pool is restricted to CC0 and public domain
sources: CC-BY would force exactly that marking.

## Companion tool

`tools/poolctl` is both the build step and the operator's curation interface: take a found image,
record its source and licence, normalise it, and emit its manifest entry. The manifest is
generated, never hand-edited — a hand-edited ordering would silently change every future
derivation.
