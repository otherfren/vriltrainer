# Contract — Pool manifest format

Published per pool version. Anyone recomputing a trial needs it, so it is a contract rather than
an internal asset.

```jsonc
{
  "version": 3,
  "created": "2026-07-20T09:14:00Z",
  "count": 512,
  "images": ["img_04c1…", "img_09fe…", "img_0b77…"],   // ascending, this order is the index
  "manifest_hash": "sha256:…"                           // over the sorted id list
}
```

## Rules

- **`images` is sorted ascending and its order *is* the index** the derivation draws against
  (R1). A different order produces different targets from the same seed, so the ordering is
  normative, not cosmetic.
- **`manifest_hash` is a plain hash over the sorted id list.** A Merkle root was specified
  originally and dropped: a tree buys inclusion proofs without a full download, and the manifest
  is published whole (D5, D17).
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
