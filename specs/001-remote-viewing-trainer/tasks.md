---

description: "Task list for Remote Viewing Trainer"
---

# Tasks: Remote Viewing Trainer

**Input**: Design documents from `/specs/001-remote-viewing-trainer/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Not blanket-mandated. Test tasks appear only where the constitution requires them —
every released contract needs at least one test (principle III) — and for the derivation, where
D7 makes shared vectors mandatory because two independent implementations must agree.

**Organization**: Grouped by user story, in the P1–P4 order fixed in D15.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1–US4, matching the user stories in spec.md

## Path Conventions

`server/` Rust service, `tools/poolctl/` Rust CLI, `client/` Angular SPA, `shared/` the contract
artefacts both implementations consume. Structure fixed in plan.md.

---

## Phase 1: Setup

**Purpose**: Repository skeleton and toolchains.

- [ ] T001 Create the workspace layout from plan.md — `server/`, `tools/poolctl/`, `client/`, `shared/vectors/`, `shared/pool/`
- [ ] T002 Create the Cargo workspace in `Cargo.toml` with members `server` and `tools/poolctl`
- [ ] T003 [P] Add server dependencies in `server/Cargo.toml` — axum, rusqlite, sha2, chacha20poly1305, serde, tracing
- [ ] T004 [P] Add pool tool dependencies in `tools/poolctl/Cargo.toml` — image, sha2, serde, clap
- [ ] T005 [P] Initialise the Angular application in `client/` with `@angular/localize`, `PathLocationStrategy` (required by D9 — `HashLocationStrategy` would collide with the access-link fragment)
- [ ] T006 [P] Write `.gitignore` covering `target/`, `node_modules/`, `dist/`, `*.sqlite*`, `.env*`
- [ ] T007 [P] Configure formatting and linting — `rustfmt.toml`, `clippy.toml`, ESLint and Prettier in `client/`
- [ ] T008 Make the GitHub repository public, completing D6 — until this is done, "open source" is an intention rather than a fact

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Everything the trial loop stands on. **No user story can begin before this phase
completes.**

**⚠️ Two items here gate the entire MVP** and are called out in D15: the curated image pool and
the derivation vectors. Neither can be worked around later.

### The derivation contract

- [ ] T009 Specify the derivation in `shared/vectors/README.md` — SHA-256 counter stream, LE64 counter, 64-bit words, rejection sampling bound, partial Fisher-Yates for decoys, final shuffle for display order, exactly as fixed in research.md R1
- [ ] T010 Generate the vector fixtures in `shared/vectors/derivation.json` — seed, pool version, expected target index, expected decoy indices, expected display order, covering pool sizes at and around rejection-sampling boundaries
- [ ] T011 Implement the derivation in `server/src/trial/derive.rs`
- [ ] T012 Add the conformance test in `server/tests/derivation_vectors.rs` reading `shared/vectors/derivation.json` — **this is the load-bearing test of the project** (D7, quickstart Scenario 2)

### The image pool

- [ ] T013 [P] Implement image normalisation in `tools/poolctl/src/normalise.rs` — fixed edge length, uniform requantisation, metadata stripped, identifier from the hash of normalised bytes
- [ ] T014 [P] Implement the annotate command in `tools/poolctl/src/annotate.rs` recording source URL, licence and attribution per image — this is the operator's curation interface, not only a build step (D17)
- [ ] T015 Implement manifest emission in `tools/poolctl/src/manifest.rs` — sorted identifier list plus a plain hash over it, per contracts/pool-manifest.md; the ordering is normative because the derivation indexes into it
- [ ] T016 Curate and normalise at least 500 freely licensed images into `shared/pool/v1.json` — the largest piece of manual work in the project, and it blocks every playable scenario (FR-012, D5)

### Storage, chain and crypto

- [ ] T017 Create the schema and migrations in `server/src/db/schema.sql` for `account`, `log_entry`, `pool_version`, `pool_image`, `handoff_code`, `account_stats`, per data-model.md
- [ ] T018 Configure SQLite in `server/src/db/mod.rs` — WAL, single writer connection, reader pool (research.md R9)
- [ ] T019 Implement the append-only chain in `server/src/log/chain.rs` — monotonic gapless `seq`, `prev_hash`, `entry_hash`, append inside one transaction
- [ ] T020 [P] Implement commitment hashing in `server/src/trial/commit.rs` — `SHA-256(s_server ‖ nonce ‖ coordinate)`; the coordinate is inside the hash, without which the reveal proves nothing about which coordinate was shown
- [ ] T021 [P] Implement token seal and open in `server/src/trial/token.rs` — XChaCha20-Poly1305, account identifier and trial sequence bound as additional authenticated data so tokens cannot be moved between accounts (research.md R7)

### Service scaffolding

- [ ] T022 Set up the axum server and routing skeleton in `server/src/http/mod.rs`
- [ ] T023 Implement `Host`-based locale selection in `server/src/http/locale.rs` — D10 selects the bundle from this header alone
- [ ] T024 Implement forwarded client address handling in `server/src/http/client_addr.rs`, trusting the header **only** from the proxy address — without this the per-address limit is inert or global, with a naive version any client forges its own address (research.md R8)
- [ ] T025 Add structured logging with a per-request correlation identifier in `server/src/http/trace.rs`, and assert no URL fragment can reach the logs (constitution IV, FR-006)

**Checkpoint**: The pool exists, both derivation vectors and their Rust implementation agree, and
the chain can be appended to. User stories may now begin.

---

## Phase 3: User Story 1 — Complete a viewing trial (Priority: P1) 🎯 MVP

**Goal**: A visitor invents a name and runs trials end to end.

**Independent Test**: In a fresh browser, enter a name, run one trial from coordinate to verdict,
start a second. Nothing else needs to exist.

- [ ] T026 [P] [US1] Implement account creation and capability tokens in `server/src/account/mod.rs` — token from a CSPRNG, at least 128 bits, only its hash stored (FR-001, FR-002, D9)
- [ ] T027 [US1] Implement `POST /api/account` in `server/src/http/routes/account.rs` per contracts/http-api.md
- [ ] T028 [US1] Implement `POST /api/trial` in `server/src/http/routes/trial.rs` — draw the coordinate and `s_server`, write the `COMMIT` entry **before responding**, return the commitment with it (FR-007, FR-013, D3)
- [ ] T029 [US1] Implement `POST /api/trial/reveal` in `server/src/http/routes/trial.rs` — combine `s_client`, derive target, decoys and order, issue token 2 carrying reveal time and expiry (FR-008, FR-009)
- [ ] T030 [US1] Implement `POST /api/trial/answer` in `server/src/http/routes/trial.rs` — evaluate, append the `RESOLVE` entry, return `s_server` and nonce (FR-010, FR-019)
- [ ] T031 [US1] Enforce the minimum viewing time in `server/src/trial/timing.rs` — reject under three seconds **before examining the chosen image**, leave the trial open; anything else turns the rule into an oracle for the target (FR-039, SC-016)
- [ ] T032 [US1] Enforce one evaluated answer per trial and the validity period in `server/src/trial/state.rs` — a speed-rejected answer does not consume the trial (FR-037, FR-038)
- [ ] T033 [P] [US1] Build the trial screen in `client/src/app/trial/trial.component.ts` — coordinate, reveal, eight images, verdict, next
- [ ] T034 [P] [US1] Implement access-link handling in `client/src/app/account/access-link.service.ts` — read the fragment, store it, clear the address bar with `history.replaceState` (FR-006)
- [ ] T035 [US1] Build the access-link panel in `client/src/app/account/access-link.component.ts` — masked by default, reveal button, copy **without** revealing, re-mask on a timeout (D9, D21)
- [ ] T036 [US1] Add the contract test for the trial endpoints in `server/tests/contract_trial.rs` — constitution principle III requires released contracts to be covered
- [ ] T037 [US1] Verify no response before the answer identifies the target, in `server/tests/no_target_leak.rs` (SC-011)

**Checkpoint**: The product is playable and its central protocol property is enforced.

---

## Phase 4: User Story 2 — Find out whether I am beating chance (Priority: P2)

**Goal**: A practitioner sees whether their results mean anything.

**Independent Test**: Run ten trials scoring zero hits; statistics appear regardless. Then confirm
the deviation arrives with its by-chance context.

- [ ] T038 [US2] Maintain `account_stats` incrementally on resolve in `server/src/stats/accumulate.rs` — completed, hits, abandoned, distinct UTC days
- [ ] T039 [P] [US2] Implement the Wilson lower bound and deviation from chance in `server/src/stats/measures.rs`
- [ ] T040 [P] [US2] Implement the exact binomial tail and its per-10,000 phrasing in `server/src/stats/by_chance.rs` — exact rather than normal-approximated, because small `n` is where the claim is loudest (research.md R3)
- [ ] T041 [US2] Implement block-wise advancement in blocks of 25 in `server/src/stats/blocks.rs` (FR-019, D17)
- [ ] T042 [US2] Implement `GET /api/stats/me` in `server/src/http/routes/stats.rs` — gate on completed-trial count alone, never on success, and always report the abandoned count (FR-015, FR-016, FR-021)
- [ ] T043 [US2] Implement `GET /api/stats/aggregate` in `server/src/http/routes/stats.rs` including both tail counts (FR-020, FR-043)
- [ ] T044 [P] [US2] Build the personal statistics view in `client/src/app/stats/personal.component.ts` — deviation always beside its by-chance line, never alone
- [ ] T045 [P] [US2] Build the aggregate view in `client/src/app/stats/aggregate.component.ts` — headline treatment that holds even when the result is exactly chance, which is the expected outcome (FR-045, D18)
- [ ] T046 [US2] Test that the statistics gate ignores success in `server/tests/stats_gate.rs` — ten trials with zero hits and ten with hits must behave identically (SC-006)

**Checkpoint**: The product measures, and measures honestly.

---

## Phase 5: User Story 3 — Satisfy myself that the game is not rigged (Priority: P3)

**Goal**: A sceptic checks a trial, then the whole record.

**Independent Test**: Complete a trial, verify it in the browser, download the log and recompute
it independently.

- [ ] T047 [P] [US3] Implement the derivation in TypeScript in `client/src/app/verify/derive.ts` — independent of the Rust implementation by design; shared code would verify itself against itself (D7)
- [ ] T048 [US3] Add the TypeScript conformance test in `client/src/app/verify/derive.spec.ts` against the **same** `shared/vectors/derivation.json`
- [ ] T049 [US3] Implement commitment verification in `client/src/app/verify/commitment.ts` using WebCrypto
- [ ] T050 [US3] Build the verification panel in `client/src/app/verify/verify.component.ts` — recompute and compare in the browser with no external tool (FR-020)
- [ ] T051 [US3] Surface verification failure visibly in `client/src/app/verify/verify.component.ts` — a verifier that only ever says "ok" has not been tested (FR-021)
- [ ] T052 [P] [US3] Implement the log export in `server/src/log/export.rs` per contracts/public-log.md — newline-delimited JSON from a sequence number
- [ ] T053 [US3] Implement `GET /api/log` and `GET /api/log/head` in `server/src/http/routes/log.rs` (FR-022)
- [ ] T054 [US3] Implement `GET /api/pool/{version}/manifest` in `server/src/http/routes/pool.rs` — without it nobody can recompute anything
- [ ] T055 [US3] Implement leaderboard eligibility in `server/src/stats/eligibility.rs` — 100 completed trials across at least three distinct UTC days (FR-040, research.md R4)
- [ ] T056 [US3] Implement positional rank assignment in `server/src/stats/ranks.rs` — the ladder from D19, withheld entirely below 200 eligible accounts and reporting progress toward it instead (FR-042)
- [ ] T057 [US3] Implement `GET /api/leaderboard` in `server/src/http/routes/leaderboard.rs` — sorted by the Wilson lower bound, which is also the primary figure displayed (FR-041, D20)
- [ ] T058 [P] [US3] Build the leaderboard view in `client/src/app/leaderboard/leaderboard.component.ts` — sort key shown as the headline number, trials, hit rate and deviation alongside
- [ ] T059 [US3] Build rank artefact rendering in `client/src/app/leaderboard/rank-card.component.ts` — trial count and by-chance frequency **inside** the image, because it travels without the page around it (FR-044, SC-015)
- [ ] T060 [US3] Add the log format test in `server/tests/contract_log.rs` — chain links, commitments match, abandoned trials are commits without resolves (SC-012)
- [ ] T061 [US3] Decide the `s_client` question recorded in contracts/public-log.md — either add it to the resolve entry so third parties can re-derive decoys, or keep the documented limit deliberately

**Checkpoint**: Level B verifiability from D2 is real rather than asserted.

---

## Phase 6: User Story 4 — Use the site in my own language (Priority: P4)

**Goal**: Two domains, two languages, one account.

**Independent Test**: Accumulate trials on one domain, switch, confirm identity and history carry
over and no second account appears.

- [ ] T062 [P] [US4] Extract translatable strings and configure locale builds in `client/angular.json` — one bundle per locale, no runtime i18n library (D10)
- [ ] T063 [P] [US4] Provide German and English message catalogues in `client/src/locale/`
- [ ] T064 [US4] Serve the locale bundle by `Host` in `server/src/http/static.rs`, one binary for both domains
- [ ] T065 [US4] Implement handoff code minting and redemption in `server/src/account/handoff.rs` — single use, roughly 30 seconds
- [ ] T066 [US4] Implement `POST /api/handoff` and `POST /api/handoff/redeem` in `server/src/http/routes/handoff.rs`
- [ ] T067 [US4] Build the language switch in `client/src/app/account/language-switch.component.ts` — carries the session via a handoff code, never the long-lived token, so the switch stays safe to stream (FR-027, D11)
- [ ] T068 [P] [US4] Add `hreflang` cross-references and suppress any automatic language redirect in `client/src/index.html` (FR-028)
- [ ] T069 [US4] Implement `DELETE /api/account/name` in `server/src/http/routes/account.rs` — self-service, authenticated by the access link, which is the only proof of ownership that exists (FR-035)
- [ ] T070 [P] [US4] Build name removal in `client/src/app/account/remove-name.component.ts`
- [ ] T071 [P] [US4] Write the data protection notice for both domains in `client/src/app/legal/` — the GDPR follows the operator and the visitor, not the interface language, so it belongs on both (FR-029, D13)
- [ ] T072 [P] [US4] Add the disclosure at name entry that the name and full trial history are public (FR-030)
- [ ] T073 [US4] Add the Impressum for the `.de` domain in `client/src/app/legal/` — a separate obligation from the GDPR notice
- [ ] T074 [US4] Test that erasure leaves the record verifiable in `server/tests/erasure.rs` — remove a name, re-verify every one of that account's entries (SC-008)

**Checkpoint**: Launch-ready.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T075 [P] Implement the per-address limit on account creation in `server/src/http/limits.rs`, depending on T024 for a real client address
- [ ] T076 [P] Implement the cap on concurrent uncompleted trials per account in `server/src/trial/limits.rs` — this is what bounds log growth, since every trial is permanent (D16, D17)
- [ ] T077 Write the nginx configuration in `deploy/nginx.conf` — TLS for both domains, `Host` forwarded unchanged, real client address set; either omission silently breaks a decision (research.md R8)
- [ ] T078 [P] Write the systemd unit in `deploy/vriltrainer.service`
- [ ] T079 Verify the backup path end to end — dump, push, and **restore into a scratch database**; an untested backup of the audit log is an untested product promise (D12)
- [ ] T080 [P] Confirm the S3 bucket is not public-read — a dump carries `s_server` for trials still in flight, which are live answers (D12, D16)
- [ ] T081 [P] Produce the seven rank artefacts as original work in `client/src/assets/ranks/` — owned outright, which is what closes the licensing question; blocks nothing before ranks first render (research.md R10)
- [ ] T082 [P] Run a simulated population of random players and confirm the aggregate lands within sampling bounds of 12.5% and the two tails stay comparable (SC-005, SC-014)
- [ ] T083 [P] Write `README.md` — what the experiment is, how to verify it yourself, and what the published record does and does not prove

---

## Dependencies & Execution Order

```
Setup (T001–T008)
   ↓
Foundational (T009–T025)   ← T016 (the pool) and T012 (the vectors) gate everything
   ↓
US1  P1  (T026–T037)  ─── MVP
   ↓
US2  P2  (T038–T046)   depends on US1 producing trials
   ↓
US3  P3  (T047–T061)   depends on US2 for the statistics it ranks
   ↓
US4  P4  (T062–T074)   independent of US2/US3; could run alongside once US1 is done
   ↓
Polish (T075–T083)
```

US4 is the only story that could overlap another — it touches the account and delivery layers
rather than the trial or statistics layers. US2 and US3 are genuinely sequential: ranks rank
statistics, and statistics count trials.

## Parallel Opportunities

- **Setup**: T003, T004, T005, T006, T007 run together
- **Foundational**: the pool chain (T013, T014, T015) runs alongside the crypto primitives (T020, T021); T016 is long and manual, so start it first and let the rest proceed beside it
- **US1**: T026 and T033 and T034 touch different layers
- **US2**: T039, T040 independently; T044 and T045 independently
- **US3**: T047 alongside T052; T058 alongside T059
- **US4**: T062, T063, T068, T070, T071, T072 are largely separate files
- **Polish**: T075, T076, T078, T080, T082, T083

## Implementation Strategy

**MVP is Phase 1 through Phase 3.** That yields a playable, protocol-correct trainer with
verifiable trials, no statistics and no leaderboard. It is demonstrable and worth showing.

**Do not ship the leaderboard before the log export.** The two are deliberately in the same phase.
A leaderboard making a significance claim without a public record anyone can recompute is exactly
the unfalsifiable psi site this design exists to avoid being. If Phase 5 has to be cut short,
launch without the leaderboard rather than with an unverifiable one (D15).

**T016 starts on day one.** Several hundred curated images cannot be compressed by better tooling,
only by starting earlier.
