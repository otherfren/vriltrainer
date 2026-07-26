# Implementation Plan: Remote Viewing Trainer

**Branch**: `001-remote-viewing-trainer` | **Date**: 2026-07-25 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-remote-viewing-trainer/spec.md`

**Design basis**: `docs/trial-protocol-decisions.md` D1–D34, binding.

## Summary

A public forced-choice remote viewing experiment. Each trial commits to its target before the
viewer chooses, using randomness neither side controls alone, and lands in an append-only public
record that anyone can download and recompute. Scoring, a rank ladder of fixed sigma bands and an
aggregate significance figure sit on top; the aggregate is expected to read exactly chance, and
the product is designed to say so.

The technical shape is a single Rust binary serving an Angular single-page application in two
build locales, backed by one SQLite file, behind the operator's existing nginx. The one
deliberate duplication is the trial derivation, implemented once in Rust and once in TypeScript,
because a client that verifies the server using the server's own code verifies nothing.

## Technical Context

**Language/Version**: Rust 1.95 (server and image tool), TypeScript 5.x on Angular (client)

**Primary Dependencies**: `axum` (HTTP), `rusqlite` (storage), `sha2` (commitment and derivation),
`chacha20poly1305` (XChaCha20-Poly1305 trial tokens), `image` (pool normalisation);
`@angular/localize` for build-time i18n. No runtime i18n library, no container runtime, no Python.

**Storage**: One SQLite database in WAL mode. It is simultaneously the application store and the
public audit log, which is why its durability is a product requirement rather than operational
hygiene (D12).

**Testing**: `cargo test` for the server and the pool tool; Angular's default runner for the
client. The load-bearing artefact is a set of language-neutral **derivation test vectors** in
`shared/vectors/`, consumed by both implementations — mandatory under D7, not optional.

**Target Platform**: Linux on an existing Hetzner host, behind an existing nginx serving other
sites. Modern browsers with WebCrypto for the client-side verifier.

**Project Type**: Web application — Rust service, Angular SPA, plus a Rust CLI tool for pool
curation.

**Performance Goals**: Trial start, reveal and answer each well under 100 ms server-side. A
first-time visitor reaches their first trial within 30 seconds of arriving (SC-001), which is a
front-end and copywriting constraint more than a server one.

**Constraints**: Deployment is two instances of one static binary over one database file (D24),
plus the published pool manifest, a per-locale static bundle and the token key. The client-side
verifier must reproduce the server's derivation byte-for-byte. The access token must never appear
in any address the server records (FR-006). Behind a shared proxy, the real client address must be
forwarded, or the admin login limiter is either inert or throttles every caller together (D17 as
amended by D30). The `Host` header is forwarded too, but since D24 the locale is a startup flag,
so a wrong `Host` costs an operator warning rather than the wrong language.

**Scale/Scope**: Designed for 10^5–10^6 trials and a leaderboard population in the hundreds to
low thousands. At a million rows the log is roughly 100 MB, which SQLite handles without
comment.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment |
|---|---|
| **I. Spec Before Code** | **Pass.** `spec.md` and this plan precede any implementation; no source exists yet. |
| **II. Simplicity First** | **Pass with three justified items**, recorded in Complexity Tracking below. Notably, a Merkle tree was rejected for both the log and the pool manifest in favour of a plain chain and a plain hash, because the full data is published anyway. |
| **III. Tests Where They Earn Their Keep** | **Pass.** Tests are not blanket-mandated, but the constitution requires coverage of released contracts — which is exactly the derivation vectors and the HTTP surface in `contracts/`. Regression tests remain mandatory per fixed bug. |
| **IV. Observable by Default** | **Pass.** Structured logs with a correlation identifier per request. Note the interaction: request logs must never contain the URL fragment (FR-006) — this is free, since fragments are not transmitted, but any client-side error reporting must be checked for the same property. |
| **V. Explicit Contracts** | **Pass.** The HTTP surface, the public log format and the pool manifest format are all specified in `contracts/`. All three are consumed by third parties, so all three are versioned. |

**Post-design re-check (after Phase 1)**: still passing. The design added no new abstraction
beyond what the table above records. The one item that moved is the pool manifest, which lost its
Merkle root during design in favour of a plain hash — a simplification, in the direction the
constitution prefers.

## Project Structure

### Documentation (this feature)

```text
specs/001-remote-viewing-trainer/
├── plan.md              # This file
├── spec.md              # What the product must do
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── http-api.md
│   ├── public-log.md
│   └── pool-manifest.md
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
server/                      # Rust service — one binary
├── src/
│   ├── trial/               # commitment, token seal/open, derivation, timing rule
│   ├── log/                 # append-only hash chain, head, export
│   ├── stats/               # Wilson bound, deviation from chance, aggregate, rank ladder
│   ├── account/             # capability tokens, handoff codes, name removal
│   └── http/                # routes, startup-flag locale (D24), forwarded-address handling
└── tests/                   # includes the derivation vectors as a conformance suite

tools/poolctl/               # Rust CLI — annotate, normalise, emit manifest
└── src/

client/                      # Angular SPA, built once per locale
└── src/app/                 # root component: masked access link, name gate, language handoff
    ├── core/                # services shared across the views, including name removal
    ├── trial/               # coordinate, reveal, choice, verdict
    ├── verify/              # independent TypeScript derivation + commitment check
    ├── stats/               # personal figures, aggregate, tail symmetry
    ├── leaderboard/         # ladder, rank artefact rendering
    └── legal/               # Impressum, Datenschutz

shared/
└── vectors/                 # derivation test vectors — the contract between the two

pool/                        # curation working tree and the published manifests
├── images.toml              # the curated catalogue poolctl reads
├── categories.toml          # the category list
└── v<N>.json                # published manifest per version, immutable once cut

docs/
└── curation-guide.md        # how the pool is built, written to be followed by anyone
```

**Structure Decision**: Three deliverables in one repository — the Rust service, the Rust
curation tool, and the Angular client — plus a language-neutral `shared/` directory. `shared/`
exists because the derivation vectors are a contract *between* the two implementations rather than
an asset of either, and burying them inside `server/` would imply the Rust side is authoritative.
It is not: agreement between two independent implementations is the evidence, and the vectors are
what they agree on.

## The curation workflow is a deliverable, not a side activity

The pool is the largest piece of manual work in the project and the only one that cannot be
automated away. It therefore needs a **documented workflow with a guide plain enough that someone
other than the author can follow it** — kept in `docs/curation-guide.md`. Two reasons: the pool
must keep growing after launch, and a rule that lives only in the operator's head is applied
inconsistently the moment there is a second pair of hands or a six-month gap.

`tools/poolctl` mechanises the parts a machine can check. The guide covers the parts it cannot.

**Categories carry most of this, but not all of it (D22).** A trial draws one image from each of
eight distinct categories, so two images of the same kind can no longer appear together. That
removes the acute problem — at realistic granularity roughly two trials in three would otherwise
have contained a confusable pair.

What categories do **not** solve is thinness within a category. Twenty near-identical landscapes
in the landscape category still produce repetitive trials, and no code can repair that either. The
guide therefore has to cover both: assigning a category consistently, and keeping variety
*inside* each one.

What the guide must settle, because each of these is a judgement a tool cannot make:

- **Where to look** — the accepted CC0 and public-domain sources, and how to confirm a licence
  rather than assume one.
- **What makes a usable target** — visually distinct, unambiguous subject, strong composition.
  Remote viewing practice favours vivid and salient imagery, and a set of eight is only meaningful
  if a viewer's impression can discriminate between them.
- **What is excluded.** Images containing legible text are out: text is a semantic marker that
  distinguishes one image from seven others, and it would also break the neutrality of a
  bilingual site. Recognisable faces are out. Anything with a licence that needs an argument is
  out.
- **Recording provenance at capture time**, never afterwards — a lost source URL is not
  recoverable, and it is what makes the licence defensible later.
- **How categories are assigned** — the fixed list, what belongs where, and what to do with an
  image that plausibly fits two. Consistency matters more than precision: a category is a drawing
  bucket, not a taxonomy.
- **When to cut a pool version**, given that versions are immutable and every trial references
  the one it ran under.

## Complexity Tracking

> Filled because Constitution Check principle II requires justification for added structure.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| The trial derivation is implemented twice, in Rust and in TypeScript | FR-023 requires the browser to verify a trial without external tools, and D7 requires that verification to be independent | A shared implementation was the main argument for a TypeScript backend. It was rejected because code checking itself against itself demonstrates nothing; the cost is that the vectors in `shared/` become mandatory rather than nice to have |
| An encrypted trial token *in addition to* the commit row | Keeps `s_server` out of the database entirely, so a backup contains no pending targets (D16, closing the exposure noted in D12) | Holding full trial state server-side is simpler, but every backup would then carry the answers to every trial in flight |
| A rank ladder of fixed sigma bands layered on the statistics | Product decision D31, superseding D23 and D19 | Positional ranks and population shares were both tried first and rejected: neither can be recomputed by a visitor from their own record and the public log, and equal shares at the two ends made the tail-symmetry argument true by construction. Fixed edges make the same comparison an empirical result; the price is that titles are no longer capped, and the eligibility floor of 100 trials across 3 distinct UTC days is the only brake |
