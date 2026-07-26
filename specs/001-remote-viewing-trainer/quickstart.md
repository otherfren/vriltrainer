# Quickstart — validating the feature end to end

Runnable scenarios that demonstrate the product works and, more importantly, that its central
claim holds. Implementation detail belongs in `tasks.md`; this is the run-and-check guide.

## Prerequisites

Rust 1.95 with cargo, and Node for the Angular build only — it does not run in production (D7).
Neither Python nor a container runtime is used anywhere.

```bash
cargo run --bin poolctl -- import        # → pool/normalised/, the build's image input (D29)
cargo build --release                    # server and tools/poolctl; the images are compiled in
cd client && npm ci && npm run build     # one bundle per locale
```

A pool is required before any trial can run, and since D29 before the server is even built — the
normalised images are compiled into the binary, so `import` comes first or the binary carries no
images and refuses to start against a real manifest. This is the gating dependency for the whole
MVP — several hundred curated images, and the only part that cannot be automated. Follow
`docs/curation-guide.md`. Every image needs a category — a trial draws one image from each of
eight distinct categories — and each category needs enough variety inside it:

```bash
cargo run --bin poolctl -- import        # takes everything pool/images.toml names into the catalogue
cargo run --bin poolctl -- check         # refuses missing provenance, duplicates; reports subject spread
cargo run --bin poolctl -- build --version 2 --out pool/v2.json   # normalises, hashes, writes a manifest
cp pool/v2.json pool/manifest.json       # what --pool names
```

A single image, outside the spec files:
`cargo run --bin poolctl -- add <file> --category <name> --de <label> --en <label> --source <url> --licence CC0`

## Scenario 1 — A trial runs (US1)

```bash
cargo run --bin server -- \
  --locale de --db dev.db --pool pool/manifest.json \
  --listen 127.0.0.1:8080 --public client/dist/client/browser
```

`--locale` has no default on purpose (D24), and without `--public` the process answers under
`/api` only, with no page to open.

Open the site, enter a name, and complete one trial. Expected: a coordinate appears before any
image; eight images appear after the reveal; a verdict follows the choice; a new trial is one
action away. The access link is visible but masked, and copyable without being revealed.

**Check what should not happen**: with the network tab open, nothing in the trial-start or reveal
response identifies the target (SC-011). The `commitment` is present from the very first
response — a proof that only appears at the end would prove nothing (D3).

## Scenario 2 — The two implementations agree (D7, mandatory)

The single most important test in the project. Both sides run the same vectors:

```bash
cargo test -p server --test derivation_vectors
cd client && npm run conformance         # the same vectors in TypeScript, no browser needed
```

The client's browser unit tests are a separate run and need to be told which browser to use:
`cd client && CHROME_BIN=$(command -v chromium) npm test -- --watch=false --browsers=ChromeHeadless`.

Expected: identical targets, decoys and display order for every vector. A divergence here surfaces
in production as verification failures on honest trials, which is expensive to diagnose and
destroys the credibility the whole design exists to build.

## Scenario 3 — A trial verifies in the browser (US3)

Complete a trial and open the verification panel. Expected: the interface recomputes
`framed(s_server, nonce, coordinate)` — that is
`SHA-256(LE64(len₀) ‖ s_server ‖ LE64(len₁) ‖ nonce ‖ LE64(len₂) ‖ coordinate)`, each field
length-prefixed (D3) — matches it against the commitment shown before the choice, re-derives the
candidate set from the seed, and reports agreement — with no external tool (FR-023).

Then corrupt a stored commitment by hand and repeat. Expected: the failure is **displayed**, not
swallowed (FR-024). A verifier that only ever says "ok" has not been tested.

## Scenario 4 — The record is independently recomputable (US3, SC-004)

```bash
# one page at a time — 1 000 entries by default, 10 000 at most; entries are immutable, so a
# page already fetched never changes. Repeat with from=<last seq + 1> until a short page arrives.
curl -s 'https://vriltrainer.com/api/log?from=1&limit=10000' >> log.ndjson
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
