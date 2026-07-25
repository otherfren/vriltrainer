# Quickstart — validating the feature end to end

Runnable scenarios that demonstrate the product works and, more importantly, that its central
claim holds. Implementation detail belongs in `tasks.md`; this is the run-and-check guide.

## Prerequisites

Rust 1.95 with cargo, and Node for the Angular build only — it does not run in production (D7).
Neither Python nor a container runtime is used anywhere.

```bash
cargo build --release                    # server and tools/poolctl
cd client && npm ci && npm run build     # one bundle per locale
```

A pool is required before any trial can run. This is the gating dependency for the whole MVP —
several hundred curated images, and the only part that cannot be automated. Follow
`docs/curation-guide.md`; the pool is curated for diversity across the whole collection, because
decoys are drawn at random and eight near-identical images would make a trial meaningless:

```bash
poolctl add <file> --source <url> --licence CC0
poolctl check                            # refuses missing provenance, duplicates; reports subject spread
poolctl build --version 1                # normalises, hashes, writes shared/pool/v1.json
```

## Scenario 1 — A trial runs (US1)

```bash
cargo run --release -- --db ./dev.sqlite --pool shared/pool/v1.json
```

Open the site, enter a name, and complete one trial. Expected: a coordinate appears before any
image; eight images appear after the reveal; a verdict follows the choice; a new trial is one
action away. The access link is visible but masked, and copyable without being revealed.

**Check what should not happen**: with the network tab open, nothing in the trial-start or reveal
response identifies the target (SC-011). The `commitment` is present from the very first
response — a proof that only appears at the end would prove nothing (D3).

## Scenario 2 — The two implementations agree (D7, mandatory)

The single most important test in the project. Both sides run the same vectors:

```bash
cargo test -p server derivation::vectors
cd client && npm test -- --include='**/verify/*.spec.ts'
```

Expected: identical targets, decoys and display order for every vector. A divergence here surfaces
in production as verification failures on honest trials, which is expensive to diagnose and
destroys the credibility the whole design exists to build.

## Scenario 3 — A trial verifies in the browser (US3)

Complete a trial and open the verification panel. Expected: the interface recomputes
`SHA-256(s_server ‖ nonce ‖ coordinate)`, matches it against the commitment shown before the
choice, re-derives the candidate set from the seed, and reports agreement — with no external tool
(FR-020).

Then corrupt a stored commitment by hand and repeat. Expected: the failure is **displayed**, not
swallowed (FR-021). A verifier that only ever says "ok" has not been tested.

## Scenario 4 — The record is independently recomputable (US3, SC-004)

```bash
curl -s https://vriltrainer.com/api/log?from=0 > log.ndjson
curl -s https://vriltrainer.com/api/log/head
```

Expected: the chain links from the first entry to the published head; every commit's commitment
matches its resolve; hits over resolves reproduce the published aggregate. Commit entries without
a resolve are the abandoned trials, and counting them gives the published abandonment rate
(SC-012).

Both randomness contributions are in the resolve entry, so re-derive a trial's target, decoys and
display order from the log and the pool manifest alone and confirm they match. No participant
cooperation is required (SC-002).

## Scenario 5 — The statistics refuse to flatter (US2)

Run ten trials scoring zero hits. Expected: statistics appear anyway — the threshold counts
trials, never success (SC-006). Reaching ten trials *with* a hit must produce the same behaviour;
if the two differ, the gate is filtering on outcome and every published figure is inflated.

Check the aggregate page shows both tails side by side (FR-043). Over a large simulated run of
random play the two counts should stay comparable — that balance is the significance test, and an
asymmetry with simulated input means a bug.

## Scenario 6 — Friction without barriers (FR-039)

Answer a trial in under three seconds. Expected: `425`, the trial stays open, and **the response
is identical regardless of which image was chosen** — otherwise the timing rule is an oracle for
the target. Answer again after three seconds: it succeeds. Answer a second time: `409`.

## Scenario 7 — Language switch keeps the session (US4)

From `vriltrainer.de` with an established history, use the switch. Expected: `vriltrainer.com` in
English, same account, same trials, no second account created (SC-007). The address bar never
shows the long-lived token — only a handoff code that is already burnt.

Behind nginx, confirm both proxy requirements from R8: without a forwarded client address the
per-address account limit is inert or global, and without an unchanged `Host` the locale
selection has nothing to select on.

## Scenario 8 — Erasure leaves the record intact (US4, SC-008)

Remove the display name, then re-run Scenario 4. Expected: every one of that account's trials is
still present and still verifies, now under the opaque identifier alone (FR-036). If any entry
fails, a name reached the hash chain and the model in `data-model.md` was not followed.
