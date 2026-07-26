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

- [X] T001 Create the workspace layout from plan.md — `server/`, `tools/poolctl/`, `client/`, `shared/vectors/`, `shared/pool/`
- [X] T002 Create the Cargo workspace in `Cargo.toml` with members `server` and `tools/poolctl`
- [X] T003 [P] Add server dependencies in `server/Cargo.toml` — axum, tokio, rusqlite, tower-http, clap, sha2, chacha20poly1305, serde, tracing.
- [X] T004 [P] Add pool tool dependencies in `tools/poolctl/Cargo.toml` — image, sha2, serde, clap
- [X] T005 [P] Initialise the Angular application in `client/` with `@angular/localize`, `PathLocationStrategy` (required by D9 — `HashLocationStrategy` would collide with the access-link fragment)
- [X] T006 [P] Write `.gitignore` covering `target/`, `node_modules/`, `dist/`, `*.sqlite*`, `.env*`
- [ ] T007 [P] Configure formatting and linting — `rustfmt.toml`, `clippy.toml`, ESLint and Prettier in `client/` **Audited 2026-07-26: Rust half done (`rustfmt.toml`, `clippy.toml`); ESLint and Prettier are absent from `client/` entirely — no config, no dependency, no lint script.**
- [ ] T008 Make the GitHub repository public, completing D6 — until this is done, "open source" is an intention rather than a fact **Audited 2026-07-26: Not answerable from the tree: `gh repo view otherfren/vriltrainer --json visibility` settles it.**

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Everything the trial loop stands on. **No user story can begin before this phase
completes.**

**⚠️ Two items here gate the entire MVP** and are called out in D15: the curated image pool and
the derivation vectors. Neither can be worked around later.

### The derivation contract

- [X] T009 Specify the derivation in `shared/vectors/README.md` — SHA-256 counter stream, LE64 counter, 64-bit words, rejection sampling bound, partial Fisher-Yates for decoys, final shuffle for display order, exactly as fixed in research.md R1
- [X] T010 Generate the vector fixtures in `shared/vectors/derivation.json` — seed, pool version, expected target index, expected decoy indices, expected display order, covering pool sizes at and around rejection-sampling boundaries, and category counts with deliberately uneven category sizes so a size-proportional bias would fail the vectors
- [X] T011 Implement the derivation in `server/src/trial/derive.rs` — the four steps of D22: eight distinct categories, one image per category, target index over 0…7, then the display shuffle
- [X] T012 Add the conformance test in `server/tests/derivation_vectors.rs` reading `shared/vectors/derivation.json` — **this is the load-bearing test of the project** (D7, quickstart Scenario 2)

### The image pool

- [X] T013 [P] Implement image normalisation in `tools/poolctl/src/normalise.rs` — fixed edge length, uniform requantisation, metadata stripped, identifier from the hash of normalised bytes (FR-011)
- [X] T014 [P] Implement the annotate command in `tools/poolctl/src/annotate.rs` recording source URL, licence and attribution per image — this is the operator's curation interface, not only a build step (D17)
- [X] T015 Implement manifest emission in `tools/poolctl/src/manifest.rs` — sorted `(id, category)` pairs plus a plain hash over them, category inside the hash (D22), per contracts/pool-manifest.md; the ordering is normative because the derivation indexes into it
- [X] T084 [P] Add the manifest format test in `tools/poolctl/tests/manifest_format.rs` — asserts ascending order, the hash over the sorted list, and that a reordered manifest is rejected; the ordering silently determines every future derivation (constitution III, contracts/pool-manifest.md)
- [X] T092 Write the curation guide in `docs/curation-guide.md` — accepted sources and how to confirm a licence, what makes a usable target, exclusions (legible text, recognisable faces, arguable licences), recording provenance at capture time, and when to cut a pool version. Written to be followed by someone other than the author, because the pool keeps growing after launch
- [X] T093 [P] Add `poolctl check` in `tools/poolctl/src/check.rs` — refuse an image with no source or licence recorded, refuse a duplicate of an existing hash, refuse an image with no category, and report images per category so a thin category is visible before it starts repeating
- [ ] T016 Curate and normalise at least 160 freely licensed images (D5, amended — was 500) into `shared/pool/v1.json` following `docs/curation-guide.md` — every image assigned a category, and **variety kept inside each category** — D22 stops two images of the same kind sharing a trial, but twenty near-identical landscapes still produce repetitive trials and no code can repair that; the largest piece of manual work in the project, and it blocks every playable scenario (FR-012, SC-010, D5) **Audited 2026-07-26: The pool is curated and passes `poolctl check`, but the path in this line is wrong: there is no `shared/pool/`, and poolctl, the curation guide and the unit each name a different manifest location.**

### Storage, chain and crypto

- [X] T017 Create the schema and migrations in `server/src/db/schema.sql` for `account`, `log_entry`, `pool_version`, `pool_image`, `handoff_code`, `account_stats`, per data-model.md — the log references the opaque account id, never the name (FR-026)
- [X] T018 Configure SQLite in `server/src/db/mod.rs` — WAL, single writer connection, reader pool (research.md R9)
- [X] T019 Implement the append-only chain in `server/src/log/chain.rs` — monotonic gapless `seq`, `prev_hash`, `entry_hash`, append inside one transaction; an abandoned trial is a commit with no resolve, so it needs no marker (FR-014)
- [X] T020 [P] Implement commitment hashing in `server/src/trial/commit.rs` — `SHA-256(s_server ‖ nonce ‖ coordinate)`; the coordinate is inside the hash, without which the reveal proves nothing about which coordinate was shown
- [X] T021 [P] Implement token seal and open in `server/src/trial/token.rs` — XChaCha20-Poly1305, account identifier and trial sequence bound as additional authenticated data so tokens cannot be moved between accounts (research.md R7)

### Service scaffolding

- [X] T022 Set up the axum server and routing skeleton in `server/src/http/mod.rs`
- [X] T023 Implement locale selection from the `--locale de|en` startup flag in `server/src/http/locale.rs` — **D24 supersedes D10 here**: two processes, one per domain, and a German process cannot serve English by accident
- [X] T024 Implement forwarded client address handling in `server/src/http/client_addr.rs`, trusting the header **only** from the proxy address — without this the per-address limit is inert or global, with a naive version any client forges its own address (research.md R8)
- [X] T025 Add structured logging with a per-request correlation identifier in `server/src/http/trace.rs`, and assert no URL fragment can reach the logs (constitution IV, FR-006)

**Checkpoint**: The pool exists, both derivation vectors and their Rust implementation agree, and
the chain can be appended to. User stories may now begin.

---

## Phase 3: User Story 1 — Complete a viewing trial (Priority: P1) 🎯 MVP

**Goal**: A visitor invents a name and runs trials end to end.

**Independent Test**: In a fresh browser, enter a name, run one trial from coordinate to verdict,
start a second. Nothing else needs to exist.

- [X] T026 [P] [US1] Implement account creation and capability tokens in `server/src/account/mod.rs` — token from a CSPRNG, at least 128 bits, only its hash stored (FR-001, FR-002, D9)
- [X] T027 [US1] Implement `POST /api/account` in `server/src/http/routes/account.rs` per contracts/http-api.md
- [X] T028 [US1] Implement `POST /api/trial` in `server/src/http/routes/trial.rs` — draw the coordinate and `s_server`, write the `COMMIT` entry **before responding**, return the commitment with it (FR-007, FR-013, D3)
- [X] T029 [US1] Implement `POST /api/trial/reveal` in `server/src/http/routes/trial.rs` — combine `s_client`, derive target, decoys and order, issue token 2 carrying reveal time and expiry (FR-008, FR-009)
- [X] T030 [US1] Implement `POST /api/trial/answer` in `server/src/http/routes/trial.rs` — evaluate, append the `RESOLVE` entry, return `s_server`, `s_client` and nonce (FR-010, FR-022)
- [X] T031 [US1] Enforce the minimum viewing time in `server/src/trial/timing.rs` — reject under three seconds **before examining the chosen image**, leave the trial open; anything else turns the rule into an oracle for the target (FR-039, SC-016)
- [X] T032 [US1] Enforce one evaluated answer per trial and the validity period in `server/src/http/routes/trial.rs`, over the `log_entry_trial_kind` constraint, with the clock in `server/src/trial/timing.rs` — a speed-rejected answer does not consume the trial (FR-037, FR-038)
- [X] T033 [P] [US1] Build the trial screen in `client/src/app/trial/trial.component.ts` — coordinate, reveal, eight images, verdict, next
- [X] T034 [P] [US1] Implement access-link handling in `client/src/app/account/access-link.service.ts` — read the fragment, store it, clear the address bar with `history.replaceState` (FR-006)
- [ ] T035 [US1] Build the access-link panel in `client/src/app/account/access-link.component.ts` — masked by default, reveal button, copy **without** revealing, re-mask on a timeout (FR-003, D9, D21) **Audited 2026-07-26: Masking, reveal and copy-without-revealing are built; re-mask on a timeout is missing — nothing resets `revealed` except an explicit close.**
- [X] T088 [US1] State plainly in the access-link panel that a lost link cannot be recovered, in `client/src/app/account/access-link.component.ts` (FR-005)
- [ ] T089 [US1] Prompt the user to save the access link again on reaching the statistics threshold, in `client/src/app/account/save-reminder.component.ts` — the first prompt arrives before anything is worth keeping (FR-004, D9) **Audited 2026-07-26: Neither the component nor any equivalent prompt at the unlock threshold exists.**
- [ ] T036 [US1] Add the contract test for the trial endpoints in `server/tests/contract_trial.rs` — constitution principle III requires released contracts to be covered **Audited 2026-07-26: No `server/tests/contract_trial.rs`. The contract's 410 branch is untested at the endpoint level.**
- [X] T037 [US1] Verify no response before the answer identifies the target, in `server/tests/no_target_leak.rs` (SC-011)

**Checkpoint**: The product is playable and its central protocol property is enforced.

---

## Phase 4: User Story 2 — Find out whether I am beating chance (Priority: P2)

**Goal**: A practitioner sees whether their results mean anything.

**Independent Test**: Run ten trials scoring zero hits; statistics appear regardless. Then confirm
the deviation arrives with its by-chance context.

- [X] T038 [US2] Maintain `account_stats` incrementally on resolve in `server/src/stats/accumulate.rs` — completed, hits, abandoned, distinct UTC days (FR-015)
- [X] T039 [P] [US2] Implement the Wilson lower bound and deviation from chance in `server/src/stats/measures.rs`
- [X] T040 [P] [US2] Implement the exact binomial tail and its per-10,000 phrasing in `server/src/stats/by_chance.rs` — exact rather than normal-approximated, because small `n` is where the claim is loudest (FR-018, research.md R3)
- [ ] T041 [US2] Implement block-wise advancement in blocks of 25 in `server/src/stats/blocks.rs` (FR-019, D17) **Audited 2026-07-26: Block-wise advancement is implemented and parameterised, but the shipped block is 10, not the 25 this line and D17 name.**
- [X] T042 [US2] Implement `GET /api/stats/me` in `server/src/http/routes/stats.rs` — gate on completed-trial count alone, never on success, and always report the abandoned count (FR-017, FR-016, FR-021)
- [X] T043 [US2] Implement `GET /api/stats/aggregate` in `server/src/http/routes/stats.rs` including both tail counts (FR-020, FR-043)
- [X] T044 [P] [US2] Build the personal statistics view in `client/src/app/stats/personal.component.ts` — deviation always beside its by-chance line, never alone
- [X] T045 [P] [US2] Build the aggregate view in `client/src/app/stats/aggregate.component.ts` — headline treatment that holds even when the result is exactly chance, which is the expected outcome (FR-045, D18)
- [ ] T046 [US2] Test that the statistics gate ignores success in `server/tests/stats_gate.rs` — ten trials with zero hits and ten with hits must behave identically (SC-006) **Audited 2026-07-26: The behaviour is tested, but as a unit test in `src`, not the named file, and the assertion only holds for the two extreme inputs chosen.**
- [ ] T085 [US2] Add the contract test for the statistics endpoints in `server/tests/contract_stats.rs` (constitution III, contracts/http-api.md) **Audited 2026-07-26: No dedicated contract test. `by_chance_per_10k`, `rank`, `eligible`, `distinct_days` and `wilson_upper` are documented in the contract and asserted nowhere.**

**Checkpoint**: The product measures, and measures honestly.

---

## Phase 5: User Story 3 — Satisfy myself that the game is not rigged (Priority: P3)

**Goal**: A sceptic checks a trial, then the whole record.

**Independent Test**: Complete a trial, verify it in the browser, download the log and recompute
it independently.

- [X] T047 [P] [US3] Implement the derivation in TypeScript in `client/src/app/verify/derive.ts` — independent of the Rust implementation by design; shared code would verify itself against itself (D7)
- [X] T048 [US3] Add the TypeScript conformance test in `client/src/app/verify/derive.spec.ts` against the **same** `shared/vectors/derivation.json`
- [X] T049 [US3] Implement commitment verification in `client/src/app/verify/commitment.ts` using WebCrypto
- [X] T050 [US3] Build the verification panel in `client/src/app/verify/verify.component.ts` — recompute and compare in the browser with no external tool and no technical knowledge (FR-023, SC-003)
- [ ] T051 [US3] Surface verification failure visibly in `client/src/app/verify/verify.component.ts` — a verifier that only ever says "ok" has not been tested (FR-024) **Audited 2026-07-26: The failure UI exists and is untested — no spec exercises a failing verification, which is the one thing this line exists to prevent.**
- [X] T052 [P] [US3] Implement the log export in `server/src/log/export.rs` per contracts/public-log.md — newline-delimited JSON from a sequence number, abandoned trials included as commits without resolves (FR-027)
- [X] T053 [US3] Implement `GET /api/log` and `GET /api/log/head` in `server/src/http/routes/log.rs` (FR-025)
- [X] T054 [US3] Implement `GET /api/pool/{version}/manifest` in `server/src/http/routes/pool.rs` — without it nobody can recompute anything
- [X] T055 [US3] Implement leaderboard eligibility in `server/src/stats/eligibility.rs` — 100 completed trials across at least three distinct UTC days, which is also what keeps short lucky runs off the board (FR-040, FR-028, SC-009, research.md R4)
- [ ] T056 [US3] Implement share-based rank assignment in `server/src/stats/ranks.rs` — the eleven bands from D23, each awarded only once `share x eligible >= 1` with no rounding up, reporting the eligible count either way (FR-042, SC-013) **Audited 2026-07-26: This line is stale, not unimplemented. D31 replaced the share-based bands with fixed sigma bands; `ranks.rs` implements those. Rewrite the line or close it.**
- [X] T057 [US3] Implement `GET /api/leaderboard` in `server/src/http/routes/leaderboard.rs` — sorted by the Wilson lower bound, which is also the primary figure displayed (FR-041, D20)
- [X] T058 [P] [US3] Build the leaderboard view in `client/src/app/leaderboard/leaderboard.component.ts` — sort key shown as the headline number, trials, hit rate and deviation alongside, each entry naming the account and its public identifier (FR-029)
- [ ] T059 [US3] Build rank artefact rendering in `client/src/app/leaderboard/rank-card.component.ts` — trial count and by-chance frequency **inside** the image, because it travels without the page around it (FR-044, SC-015) **Audited 2026-07-26: Nothing shareable is generated at all — the SVG badges carry neither trial count nor by-chance figure.**
- [X] T060 [US3] Add the log format test in `server/tests/contract_log.rs` — chain links, commitments match, abandoned trials are commits without resolves, and the aggregate recomputes from the file alone (SC-012, SC-004)
- [ ] T086 [US3] Add the contract test for the leaderboard and pool-manifest endpoints in `server/tests/contract_leaderboard.rs` (constitution III) **Audited 2026-07-26: Named file never created; the leaderboard endpoint has no test under `server/tests`, only in-module unit tests.**
- [X] T061 [US3] Include `s_client` in the resolve entry in `server/src/log/export.rs` and `server/src/log/chain.rs` — without it only the participant can re-derive the decoys, and SC-002 promises an independent party can

**Checkpoint**: Level B verifiability from D2 is real rather than asserted.

---

## Phase 6: User Story 4 — Use the site in my own language (Priority: P4)

**Goal**: Two domains, two languages, one account.

**Independent Test**: Accumulate trials on one domain, switch, confirm identity and history carry
over and no second account appears.

- [X] T062 [P] [US4] Extract translatable strings and configure locale builds in `client/angular.json` — one bundle per locale, no runtime i18n library, both offering identical functionality (FR-030, D10)
- [X] T063 [P] [US4] Provide German and English message catalogues in `client/src/locale/`
- [X] T064 [US4] Serve the locale bundle fixed at startup in `server/src/http/static.rs` — one binary, two processes, two systemd units, two nginx upstreams (D24)
- [X] T065 [US4] Implement handoff code minting and redemption in `server/src/account/handoff.rs` — single use, roughly 30 seconds
- [X] T066 [US4] Implement `POST /api/handoff` and `POST /api/handoff/redeem` in `server/src/http/routes/handoff.rs`
- [X] T067 [US4] Build the language switch — implemented in `client/src/app/app.component.ts` (`switchTo`) rather than a component of its own: it is two links in the header, and a component wrapping two links is a file to open before you can read them — carries the session via a handoff code, never the long-lived token, so the switch stays safe to stream (FR-031, D11)
- [X] T068 [P] [US4] Add `hreflang` cross-references and suppress any automatic language redirect in `client/src/index.html` (FR-032)
- [X] T069 [US4] Implement `DELETE /api/account/name` in `server/src/http/routes/account.rs` — self-service, authenticated by the access link, which is the only proof of ownership that exists (FR-035)
- [X] T070 [P] [US4] Build name removal in `client/src/app/account/remove-name.component.ts`
- [X] T071 [P] [US4] Write the data protection notice for both domains in `client/src/app/legal/` — the GDPR follows the operator and the visitor, not the interface language, so it belongs on both (FR-033, D13)
- [ ] T072 [P] [US4] Add the disclosure at name entry that the name and full trial history are public (FR-034) **Audited 2026-07-26: The name being public is disclosed at entry; the "and the full trial history" half of FR-034 is not.**
- [X] T073 [US4] Add the Impressum for the `.de` domain in `client/src/app/legal/` — a separate obligation from the GDPR notice
- [X] T074 [US4] Test that erasure leaves the record verifiable in `server/tests/erasure.rs` — remove a name, re-verify every one of that account's entries, which stay under the opaque identifier (FR-036, SC-008)
- [ ] T087 [US4] Add the contract test for the account and handoff endpoints in `server/tests/contract_account.rs` (constitution III) **Audited 2026-07-26: Named file absent. The tests are `#[cfg(test)]` modules inside the route files, so the contract itself is not what is being tested.**
- [X] T090 [US4] Test that a language switch preserves the account and creates no duplicate, in `server/tests/handoff_identity.rs` (SC-007)

**Checkpoint**: Launch-ready.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] ~~T075 [P] Implement the per-address limit on account creation in `server/src/http/limits.rs`, depending on T024 for a real client address~~ **Reverted 2026-07-26 (D30): the limit and `server/src/http/limits.rs` are removed. A test now asserts its absence.**
- [X] ~~T076 [P] Implement the cap on concurrent uncompleted trials per account in `server/src/http/limits.rs`, beside the per-address limit D17 names in the same breath — this is what bounds log growth, since every trial is permanent (D16, D17)~~ **Reverted 2026-07-26 (D30): the cap is removed. A test now asserts its absence.**
- [X] T077 Write the nginx configuration in `deploy/nginx.conf` — TLS for both domains, `Host` forwarded unchanged, real client address set; either omission silently breaks a decision (research.md R8)
- [X] T078 [P] Write the systemd unit in `deploy/vriltrainer@.service` — a template, one instance per locale (D24)
- [ ] T079 Verify the backup path end to end — dump, push, and **restore into a scratch database**; an untested backup of the audit log is an untested product promise (D12) **Audited 2026-07-26: The local half is built and exercised hourly (`deploy/backup.sh`, `verify_log`): snapshot, chain walk, encrypt, decrypt, restore into a scratch database, all verified 2026-07-26. Push is untested — no off-site endpoint is configured yet.**
- [ ] T080 [P] Confirm the S3 bucket is not public-read — a dump carries `s_server` for trials still in flight, which are live answers (D12, D16) **Audited 2026-07-26: No bucket exists yet. Blocked on the off-site half of T079.**
- [X] T081 [P] Produce the rank artefacts as original work in `client/public/rank/` — **done, and there are eleven** (D23). Original pixel work, owned outright, which is what closes the licensing question (research.md R10)
- [ ] T082 [P] Run a simulated population of random players and confirm the aggregate lands within sampling bounds of 12.5% and the two tails stay comparable (SC-005, SC-014). **Include an adversarial farmer** splitting a fixed trial budget across many accounts, so D27's claim that splitting beats concentrating is measured rather than argued **Audited 2026-07-26: No simulation harness in the tree, and no recorded run of one.**
- [ ] T094 [P] Verify the draw is unbiased across categories in `server/tests/category_bias.rs` — with deliberately uneven category sizes, the target must land on each displayed position equally often, and always choosing the largest category's image must score 12.5% (FR-046, SC-017) **Audited 2026-07-26: Written and passing, but the clause that matters is untested: nothing measures where the target lands in `display_order`, only that the shuffle is a permutation.**
- [X] T083 [P] Write `README.md` — what the experiment is, how to verify it yourself, and what the published record does and does not prove
- [ ] T091 [P] Measure time from cold arrival to first completed trial and confirm it stays under 30 seconds (SC-001) — a manual acceptance check, not an automated test — **through the combined landing-and-name screen**, which is now the first thing a visitor sees **Audited 2026-07-26: Needs a timed manual run, and T095 has to land first since the measurement is specified through that screen.**

### Added 2026-07-25, from the launch-plan grilling

- [ ] T095 Make `/` a combined landing-and-name screen in `client/src/app/` — the premise and the 12.5% above the name field, one button into the trial. Today `/` redirects to `/trial` and a cold visitor's first screen is a form demanding a public name with nothing explaining the site (SC-001, FR-001) **Audited 2026-07-26: The premise text and the single button exist inside the name gate, but `/` still redirects to `/trial` and the 12.5% is not shown.**
- [X] T096 Implement the name state machine in `server/src/account/name.rs` — `pending` -> `approved` | `rejected`, the opaque identifier shown in place of an unapproved name, the last approved name retained across a rename (FR-047, FR-048, SC-018, D25)
- [X] T097 Port `checkDisplayName` to `server/src/account/name_filter.rs` and enforce it on name submission — the client copy is UX only, and a rule checked only in the client is not checked (D25)
- [X] T098 Implement the public admin API in `server/src/http/routes/admin.rs` — list pending, approve, reject. **Reversible operations only**; anything destructive stays a CLI subcommand behind SSH (D25)
- [X] T099 Implement one admin key with its hash in the database, and `server admin-key --rotate` printing the key once — in the database rather than the environment file so rotation needs no restart (D25)
- [ ] T100 Implement the rename endpoint with its rate limit in `server/src/http/routes/account.rs`, and confirm a rejected name does not consume it (FR-048) **Audited 2026-07-26: Cooldown logic and its tests exist in `server/src/account/name.rs`; the HTTP route was never added.**
- [X] T101 Make the thresholds configuration in `server/src/config.rs` and report them in the responses that depend on them — statistics unlock, eligibility floor, band edges (FR-050, D26)
- [ ] T102 Recompute ranks on a ~15-minute background task and materialise them, exposing when they were last computed (D23) **Audited 2026-07-26: Recomputation happens, but from the read path (`ranks::ensure_fresh`), not from a background task.**
- [X] T103 Enforce the D24 append discipline in `server/src/db/mod.rs` — `BEGIN IMMEDIATE` before reading the chain head, `busy_timeout`, `UNIQUE` on sequence number and `prev_hash`, chain walk at startup (R9, D24)
- [X] T104 Rewrite `deploy/nginx.conf` for two upstreams and two `server` blocks, and delete the claim that `Host` selects the language build — it no longer does (D24)
- [X] T105 [P] Add the empty state to the statistics page — a grey empty chart and one honest line while there are not enough trials. Today the page hardcodes 148,213 trials and 720 players and has no n=0 behaviour
- [ ] T106 [P] State the multi-accounting position on the statistics page in the site's own voice (D27) **Audited 2026-07-26: Stated in the spec and the decisions, never on the stats page in the site's own voice.**
- [ ] T107 [P] Add the DSA notice-and-action line to the Impressum and footer — what to report and where. The Impressum contact is the mechanism; there is no report button in v1 **Audited 2026-07-26: Present only in `datenschutz.component.ts`; neither named location carries it.**
- [ ] T108 [P] Make the log export prominent and invite readers to keep their own copy — this is what makes the record survive the service (D12 bus factor) **Audited 2026-07-26: The export is linked; the "invite readers to keep their own copy" half is absent and the prominence was never changed.**
- [X] T109 [P] Remove the demo data from the client — `demoMode`, the demo pool, the hardcoded access key, the fabricated player and leaderboard figures
- [X] T110 [P] Delete `client/public/ui/flag-de.svg` and `globe-en.svg`; the language switch is DE and EN as text
- [ ] T111 Emit structured request logs in `server/src/http/trace.rs` — correlation id, matched route pattern (not the raw path, which can carry an account id), status, duration, locale; assert no fragment and no full referrer reaches them (FR-051, D28) **Audited 2026-07-26: Correlation id, status and duration are emitted; the matched route pattern (the anti-account-id requirement) and the locale field are not.**
- [ ] T112 Add `daily_metric(day, locale, metric, count)` and increment it in process — page views, accounts created, trials started/completed/abandoned, names submitted/approved, proofs opened, log downloads (FR-052, D28)
- [ ] T113 Count unique visitors with a daily-rotating in-memory salt and an in-memory set, persisting only the day's count and discarding salt and set at rollover (FR-052, D28)
- [ ] T114 Add `server metrics --since` as a CLI subcommand. The public admin API stays name approval only (D25, D28) **Audited 2026-07-26: No subcommand, and no metrics store for it to read (T112).**
- [ ] T115 [P] Set a short retention on the nginx access log and state it in the privacy notice — it is the only place a visitor's address is written down (D28) **Audited 2026-07-26: The 7-day retention is published in the privacy notice and enforced nowhere — no `access_log` directive and no logrotate config in `deploy/`.**

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
US3  P3  (T047–T061)   split: verification (T047–T054, T060) is independent of US2;
                       the leaderboard (T055–T059) ranks US2's statistics and cannot precede them
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
