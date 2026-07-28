# Launch plan

Everything still standing between this repository and a live `vriltrainer.de` / `vriltrainer.com`.

Written 2026-07-25, revised the same day after a decision review. This is a working document: tick
things off, and when reality disagrees with it, change the document rather than the memory of it.

The authoritative task list remains
[`specs/001-remote-viewing-trainer/tasks.md`](../specs/001-remote-viewing-trainer/tasks.md). This
plan is broader — it covers what tasks.md has no task for: hosting, DNS, TLS, moderation, legal,
and the day itself.

---

## 1. Where this actually is

Rewritten 2026-07-26, after checking every claim below against the code rather than against this
document. The version this replaced described a repository with no HTTP layer, no database, no
pool and no English build. All four had been built by then, and reading the old text would have
sent anyone straight back to work that was finished.

**Live.** Both domains serve, `vriltrainer@de` and `vriltrainer@com` run against one database, and
the site has taken real trials from real accounts.

**Done and load-bearing:**

- The derivation contract. `shared/vectors/README.md` is normative, `shared/vectors/derivation.json`
  holds the fixtures, and Rust and TypeScript agree on all seven cases. This is the hardest thing
  in the project and it is finished.
- The server. Axum router, SQLite with WAL and a migration runner, the two-writer append discipline,
  the chain walk at startup, graceful shutdown, a health endpoint. The whole trial loop — commit
  before responding, reveal, answer — with the minimum viewing time checked before the chosen image
  is ever read.
- Statistics and the leaderboard: incremental accumulation, Wilson bounds, an **exact** binomial
  tail, block-wise advancement, eligibility, the sigma rank ladder, a paginated board.
- The public log: NDJSON export, `GET /api/log`, `/api/log/head`, per-version pool manifests, and a
  format test that recomputes the aggregate from the downloaded file alone.
- Names and moderation: the pending/approved/rejected/erased state machine, the server-side name
  filter, permanent erasure, the reversible-only admin API with a rotatable hashed key.
- The image pool: 500 curated images across 19 categories, cut as v1, compiled into the binary (D29).
  `poolctl` normalises, annotates, checks and builds; a reordered manifest is rejected.
- Both languages. Extraction, two catalogues, two builds, two instances, `hreflang` without a
  redirect. The English copy is written rather than translated, jokes included.
- The client talks to the real server. `demoMode` is gone from the tree, access links are read from
  the fragment and cleared from the address bar, and every endpoint has a pending and a failure state.
- The audit log leaves the box hourly, verified: export, rebuild into a scratch database, walk the
  chain, then push to S3. The bucket answers 403 anonymously and the uploading key can only put.
- The repository is public under AGPL-3.0 (T008, confirmed from outside 2026-07-26).

**The real remaining work**, in rough order of how much it would embarrass us:

- The rank artefact of FR-044 does not exist: nothing shareable is generated, so the trial count and
  the by-chance figure never travel with a badge (T059).
- Nothing has run a simulated population through the statistics, so D27's claim that splitting a
  trial budget across accounts beats concentrating it is argued and not measured (T082).
- Copy that is decided and not on the page: the multi-accounting position, the DSA notice-and-action
  line, the invitation to keep your own copy of the log (T106–T108).
- Token key rotation is undesigned, and nothing watches uptime, disk or errors (§ J).

**Closed on 2026-07-28**, all previously on this list: the client suite runs and CI enforces the
conformance check (§ K); the traffic counters and `server metrics --since` exist (T112–T114); `/`
serves the screen rather than redirecting to it, with the 12,5 % above the name field (T095); the
rename is mounted at `PUT /api/account/name` (T100); and the four named contract-test files exist
and go in through the wire (T036, T085–T087).

**Two lines below are stale rather than unfinished:** the concurrent-trial cap and the per-address
account limit were **removed by D30**, not left undone.

---

## 2. Decisions taken 2026-07-25

Recorded in full as D23–D27 and amended into D5, D10, D19, R9 and the spec. Summarised here
because the rest of this plan assumes them.

| | Decision | Recorded in |
|---|---|---|
| 1 | Pool is **160 images** at launch, grown afterwards. D5's original sizing rationale was wrong and is corrected in place. | D5, SC-010 |
| 2 | Leaderboard ships in v1. Ranks are **shares**, each band awarded once `share x eligible >= 1`, recomputed every ~15 min. | D23, FR-042 |
| 3 | `/` is a **combined landing-and-name screen**. | T095 |
| 4 | **Rename** is rate-limited; names are **not unique**; **erasure is permanent** and the account plays on under its identifier. | FR-035, FR-047-049, D25 |
| 5 | **Two processes, one machine, one database.** Locale is a startup flag, not the `Host` header. | D24, R9 |
| 6 | Multi-accounting is **absorbed, not defended**. No scoring rule fixes it. | D27 |
| 7 | Thresholds start **low** and are **configuration**, published and announced before they move. | D26, FR-050 |
| 8 | **No standalone verifier in v1.** In-browser only, and the interface must not claim third-party independence. | SC-002 |
| 9 | Manifests permanent **while the service runs**; image bytes replaceable. Backups are private; publicly say only that they run. | D5 |
| 10 | Statistics empty state is an **empty grey chart and one honest line**. | T105 |
| 11 | Reporting via the **Impressum contact**. Names are **pre-approved**; masked in public until then, always visible to their owner. | D25, T107 |
| 12-13 | Admin API is **public**, one key, hash in the database, `--rotate` with no restart, **reversible operations only**. | D25, T098-T099 |
| 14 | **No age restriction** of any kind. Deliberate, not an oversight. | this document |
| 15 | **No flags** on the language switch — DE and EN as text. | T110 |
| 17 | **Logging** is operational lines with no visitor identifier, daily aggregate counters, and a unique-visitor count behind a daily-rotating salt that is never persisted. | D28, FR-051/052 |
| 16 | **Bus factor accepted.** Registrar login to a trusted person; mirroring pushed onto users. | this document, T108 |

---

## 3. The critical path

```
poolctl (2d) ──> curate 160 images (1–2 weeks, manual) ─────────────┐
                                                                     ├──> integration ──> launch
server foundation (1w) ──> trial loop (1w) ──> stats + log (1.5w) ──┤          (1w)
names + admin API (3d) ─────────────────────────────────────────────┤
client re-wiring (1w) ──────────────────────────────────────────────┘
English: i18n + handoff + written copy (2w, parallel)
ops + legal (1w, parallel)
```

**This estimate has been overtaken by events** and is kept only because the shape of it was right.
Every branch above is built and live, and the pool ended at 500 images rather than the 160 the
estimate assumed — the manual work that was supposed to be the long pole was not, in the end, what
took the time.

What is left is not on this diagram at all. It is the quality gates in § K, the metrics in § A, and
the two client screens in § G, none of which sit on a critical path to anything: they are the work
that should have happened before launch and now has to happen after it.

---

## A. Server foundation

- [X] Real dependencies in `server/Cargo.toml`: axum, tokio, rusqlite, tower-http, tracing,
      tracing-subscriber, clap. (T003) All seven, declared through the workspace.
- [X] `server/src/db/schema.sql` — `account`, `log_entry`, `pool_version`, `pool_image`,
      `handoff_code`, `account_stats`, plus `admin_key` and the name state from § F. The log
      references the opaque account id, never the name (FR-026). (T017) All seven tables exist and
      `log_entry` has no name column at all, so FR-026 holds structurally rather than by discipline.
- [X] `server/src/db/mod.rs` — WAL, reader pool, and the **two-writer discipline**: `BEGIN
      IMMEDIATE` before reading the chain head, `busy_timeout`, `UNIQUE` on sequence number and
      `prev_hash`, chain walk at startup. (T018, T103, R9, D24) WAL is asserted after being set,
      not merely requested, and a fork-under-load test exercises the two writers.
- [X] Migration runner. Retrofitting a version table onto a live audit log is miserable. Two
      migrations have already run through it, including the pool-binding one.
- [X] CLI: `--db`, `--pool`, `--listen`, and `--locale de|en`. (D24) `--locale` deliberately has no
      default, and a test holds that open.
- [X] `server/src/http/mod.rs` — axum router skeleton. (T022) Ten resource modules merged.
- [X] Locale fixed at startup, not from `Host`. (T023, T064) `host_matches` warns and nothing
      branches on it.
- [X] Forwarded client address, trusted **only** from the proxy address. (T024) Walks
      `X-Forwarded-For` right to left and stops at the first untrusted hop. Note what it now guards:
      D30 removed the per-address account limit, so the only remaining consumer is the admin rate
      limiter.
- [ ] Structured logs with a request id, and an assertion that no URL fragment reaches them. (T025,
      T111) The matched route pattern, never the raw path — a path can carry an account id.
      **Half done:** the request id, the fragment assertion and its positive control are in place,
      but the span still logs the raw path. `MatchedPath` appears nowhere, so the account id in
      `/admin/names/{id}/approve` lands in the access log verbatim. The locale field is also absent.
- [ ] Daily aggregate counters, unique visitors behind a daily-rotating in-memory salt, and
      `server metrics --since` to read them. No per-visitor row anywhere. (T112-T114, D28)
      **Untouched.** No `daily_metric` table, no metrics module, and `Cli` is a flat struct with no
      subcommand, so `server metrics` cannot even parse.
- [X] Graceful shutdown, so a deploy does not truncate a write to the audit log. SIGTERM and SIGINT.
- [X] Health endpoint for the monitor in § J. Reads the chain head rather than returning a
      constant, and answers 503 on a database it cannot read.

## B. The trial loop

- [X] `POST /api/account` — CSPRNG token, >=128 bits, only its hash stored. (T026, T027) 256 bits,
      and a test scans the database file to prove the token itself never reaches it.
- [X] `POST /api/trial` — draw coordinate and `s_server`, write `COMMIT` **before responding**.
      (T028) The token is sealed against the `seq` the append returned, so it cannot exist before
      the row does. A test reads the log before it reads the response.
- [X] `POST /api/trial/reveal` — combine `s_client`, derive, issue token 2. (T029) The response
      struct carries images and token only, with a comment forbidding additions.
- [X] `POST /api/trial/answer` — evaluate, append `RESOLVE`, return `s_server`, `s_client`, nonce.
      (T030) The three secrets are released only after the append, and the statistics update rides
      in the same transaction.
- [X] Minimum viewing time, checked **before** the chosen image is examined, or the rule becomes an
      oracle for the target. (T031) The gate is handed the token and the clock and nothing else, so
      it structurally cannot see the choice; a test compares the refusal's raw bytes across
      candidates.
- [X] One evaluated answer per trial; a speed-rejected answer does not consume it. (T032) A unique
      index on `(trial_id, kind)`, so the race resolves to 409 rather than to two resolves.
- [x] ~~Concurrent-uncompleted-trial cap.~~ **Removed by D30** (T076 reverted). An account may hold
      unlimited open trials, and there is a test asserting exactly that. Log growth is bounded by
      the expiry clock instead.
- [x] ~~Per-address limit on account creation.~~ **Removed by D30** (T075 reverted). The route takes
      no client address at all.
- [ ] Contract test for the trial endpoints. (T036) **Half done:** there is no
      `server/tests/contract_trial.rs`, but substantial in-module coverage drives the real router
      rather than calling handlers. The gap that matters is the expiry branch: `Gone` is returned
      from six places and no test asserts a 410.
- [X] Test that no response before the answer identifies the target. (T037) Two of them, plus a
      third covering the refusal path.
- [X] Test the draw is unbiased across categories with uneven category sizes. (T094) Both halves:
      the target slot is uniform under lopsided categories, and always picking the largest
      category's image scores chance. A fixture test keeps a 10:1 pool in the shared vectors so the
      property cannot be lost silently.

## C. Statistics and the leaderboard

- [X] `account_stats` maintained incrementally on resolve. (T038) Upsert in the same transaction as
      the log append, with a rebuild-from-log path and a staleness repair beside it.
- [X] Wilson lower bound and deviation from chance, server side. (T039) Upper bound too.
- [X] **Exact** binomial tail for the per-10,000 phrasing. The client uses Abramowitz & Stegun
      7.1.26, which is fine for a live figure and not for the published one; small `n` is exactly
      where the claim is loudest. (T040) Exact log-space summation, no normal approximation on the
      server anywhere, two-sided on the side the result fell.
- [X] Block-wise advancement. (T041) Built and parameterised. **The shipped block is ten, not the
      twenty-five this line used to name** — the number moved under D26, the mechanism did not.
- [X] `GET /api/stats/me` — gate on completed count alone, never on success; always report the
      abandoned count. (T042) Abandoned appears in the locked shape as well as the unlocked one, so
      selective abandonment cannot hide behind the gate.
- [X] `GET /api/stats/aggregate` including both tail counts. (T043) Plus the full band
      distribution, so the chart and the ladder read off one axis.
- [X] Eligibility: the configured floor across >=3 distinct UTC days. (T055)
- [X] Rank assignment. (T056) **Not share-based any more:** D31 replaced D23's
      `share x eligible >= 1` with fixed sigma edges, and the implementation takes only the
      deviation and the thresholds — the population does not appear in the signature. Every band
      exists at every population, so `band_unlocks_at` fell away with the share model.
- [ ] **Background recomputation every ~15 min**, materialised, with a last-computed timestamp on
      the board. (T102) **Half done:** materialisation and the timestamp are there and published as
      `ranks_updated_at`, but recomputation is triggered from the read path, not by a timer. The
      code labels itself a stand-in for this line.
- [X] Thresholds as configuration, reported in the responses that depend on them. (T101, D26)
      One exception worth writing down: `min_view_seconds` is a threshold the trial endpoint
      enforces and no trial response publishes.
- [X] `GET /api/leaderboard`, sorted by the Wilson lower bound. (T057) One sort key serves both
      place assignment and the page read, so a page boundary cannot reorder anyone.
- [X] Pagination on `/api/leaderboard`. Implemented with a default of 20 and a clamp, and
      `contracts/http-api.md` now documents `?offset=&limit=` — both halves of this line.
- [X] Test that the statistics gate ignores success. (T046) With a caveat: it is an in-module test
      rather than the named file, and it compares the two extreme inputs only.
- [ ] Contract tests for statistics and leaderboard. (T085, T086) Neither named file exists.
      In-module coverage is broad, but `by_chance_per_10k`, `wilson_upper`, `distinct_days`, `rank`
      and `eligible` are documented in the contract and asserted by no test at all.
- [ ] Adversarial farmer in the simulated population. (T082, D27) No harness anywhere in the tree.
      D27's claim that splitting beats concentrating stays argued rather than measured.

## D. The public log and verification

- [X] `server/src/log/export.rs` — NDJSON from a sequence number, abandoned trials included as
      commits without resolves. (T052) Included by construction: the module has no filter path, and
      abandonment is the absence of a resolve rather than a flag.
- [X] `s_client` in the resolve entry, or only the participant can re-derive the decoys and SC-002
      is false. (T061) It is inside the hash preimage, and a test re-derives a whole draw from
      `s_server` and `s_client` taken back out of the downloaded file.
- [X] `GET /api/log` and `GET /api/log/head`. (T053) Paging, cacheable finished pages, and public
      access without a token.
- [X] `GET /api/pool/{version}/manifest`, answering for **every** version for as long as the
      service runs. (T054, D5) Held hard: a missing earlier version is treated as a fault, not a
      404, and an edited or mislabelled manifest file is refused.
- [X] Log format test: chain links, commitments match, abandoned trials are commits without
      resolves, aggregate recomputes from the file alone. (T060) All four in one test, plus two
      more: dropping a trial is detectable, and a partial download cannot pass itself off as whole.
- [ ] Make the export prominent and invite readers to keep a copy. (T108) The link exists in the
      footer and nothing more. No string anywhere invites a reader to keep their own copy, which is
      the half of this line that answers the bus factor.
- [ ] ~~Standalone verifier.~~ **Not in v1** (decision 8). SC-002 has been reworded to promise the
      published procedure rather than a shipped tool, and the interface must not claim third-party
      independence it does not have.

## E. The image pool and poolctl

- [X] `tools/poolctl/Cargo.toml` dependencies: image, sha2, serde, clap. (T004) Plus a path
      dependency on the server crate, so the manifest type has one definition rather than two.
- [X] `normalise.rs` — fixed edge length, uniform requantisation, metadata stripped, id from the
      hash of the normalised bytes. (T013) 512 px, 5 bits per channel. Metadata stripping is
      structural — only computed pixels are written — and a test feeds it an EXIF fixture and
      requires the id not to move and the chunk list to be exactly IHDR/IDAT/IEND.
- [X] `annotate.rs` — source URL, licence, attribution per image. (T014) Narrowed on purpose: only
      CC0, public domain, Unsplash and Pexels are admissible, so attribution is stored as the
      operator's defence rather than rendered.
- [X] `manifest.rs` — sorted `(id, category)` pairs, hash over them, category inside the hash. The
      ordering is normative: it silently determines every future derivation. (T015) The hash rule
      lives in the server crate, so there is one copy of it.
- [X] `check.rs` — refuse missing source, licence or category; refuse duplicate hashes; report
      images per category so a thin category is visible before it repeats. (T093)
- [X] Manifest format test, including that a reordered manifest is rejected. (T084) It also proves
      the reordering would have changed what a seed draws, which is why the rule exists.
- [X] **Curate the pool.** (T016) **500 images across 19 categories**, not the 160 across 20 this
      line asked for — the floor was overshot. Repo copy and live copy carry the same hash and the
      same ids.
- [ ] Image withdrawal path: a withdrawn image serves a placeholder; the manifest and the id stay,
      so every past trial remains verifiable. (D5) Nothing exists: an id this build does not carry
      returns a plain 404, and there is no withdrawal flag in the catalogue or in `poolctl`.
- [X] Serving path — **decided the other way** and written down as D29: images are compiled into
      the binary and served by the app, not by nginx from a directory. `--pool` still passes the
      manifest, which is what made this look unresolved. Consequence to keep in mind: a pool change
      means a rebuild and a version cut.
- [ ] Attribution page listing every image, source and licence. The premise weakened rather than
      the work getting done: CC-BY is refused pool-wide, so nothing in the pool legally requires a
      visible credit. The human-readable page still does not exist, and `pool/catalogue.json` —
      source and licence per image — is in the repository but not published.

## F. Accounts, names and moderation

- [X] Name state machine: `pending` -> `approved` | `rejected`. Public surfaces show the most
      recently approved name in clear text and mask anything else with a fixed-length mask, beside
      the public id; the holder always sees the name they chose, with its state.
      (T096, FR-047, D25) Approval is the only writer of the public name column, so an unreviewed
      name cannot reach a public surface by any other path.
- [X] Port `checkDisplayName` to the server and enforce it on submission. The client copy is UX
      only and says so at the top of the module. (T097) One nit: the disclaimer sits in the
      component's doc comment, not at the top of `display-name.ts` itself.
- [X] Rename, rate-limited. A rejected name does not consume the limit; the last approved name
      stays displayed until a replacement clears. (T100, FR-048) Mounted at `PUT /api/account/name`
      on 2026-07-28 and documented in the contract. The cooldown answers `429` with `Retry-After`
      rather than `400`: nothing was wrong with the name, the request was early. Both refusal paths
      — the pre-filter's and the reviewer's — leave the turn unspent, which is what the route's
      tests exist to hold.
- [X] Erasure: permanent, account plays on under the opaque id, no new name. (FR-035) Permanence is
      enforced on the way back in: a submission from an erased account is refused.
- [X] Rejected names discarded, not retained. The rejected string is cleared from both columns and
      kept out of journald.
- [X] Test that erasure leaves the record verifiable. (T074) Plays trials over HTTP, downloads the
      log through the real endpoint, and verifies from those bytes.
- [X] Public admin API: list pending, approve, reject — **reversible only**. Everything
      destructive stays CLI behind SSH. (T098, D25) Decisions are keyed on the name string the
      reviewer actually read, with a 409 if it moved under them.
- [X] One admin key, hash in the database, no restart. (T099) Rotation retires every previous key
      in one transaction. **Deviation:** it is a separate `admin_key` binary rather than
      `server admin-key --rotate`, because `--locale` has no default under D24.
- [X] Rate limit the admin endpoint, constant-time key comparison, excluded from the nginx cache
      rule, and named explicitly in the § K security review — it is the only privileged surface in
      the system. The limit is checked before any database work, and the comparison has no early
      exit. The § K review itself is still outstanding.
- [ ] Disclosure at name entry that name and history are public. (T072) The gate says the name goes
      on the public board. It does not say the name appears only after review, and it does not say
      the full trial history is public — the second half of FR-034.
- [X] Handoff codes for the language switch, single use, ~30 s. (T065-T067) Redeem burns and looks
      up in one statement, so single use survives a race.
- [X] Test that a language switch preserves the account and creates no duplicate. (T090) Two states
      with different locales over one shared database file.
- [ ] Backlog, not v1: re-run the filter over existing names when the blocklist is extended.
      Otherwise it only ever applies to accounts created after the last edit.
- [ ] **Not needed:** name uniqueness. The public identifier distinguishes accounts (FR-049).
      Accepted consequence: two accounts can both be `otherfren`, so impersonation is possible and
      the id is the only defence — which is why FR-029 forces it to be shown alongside.

## G. Client: replace the demo with the real thing

- [ ] `/` becomes the combined landing-and-name screen. (T095) The premise copy and the single
      button live in the gate, but `/` still redirects to `/trial` and the 12,5 % is not on that
      screen. Everything else in this section landed; this one did not.
- [X] `ApiService` — replace the local derivation with real HTTP calls. **Not a flag flip** — the
      flow differs, and the proof panel must be re-pointed at server-returned values. Done: only
      `s_client` is still drawn locally, which is the point of it.
- [X] Real access-link handling: read the fragment, store it, clear the address bar with
      `history.replaceState`. (T034)
- [X] Persist the account locally so a reload does not lose it. `localStorage` with an in-memory
      fallback.
- [ ] Re-prompt to save the access link at the statistics threshold. (T089) Nothing fires at the
      threshold. The always-available login dialog is not the same thing: the first prompt arrives
      before there is anything worth keeping.
- [ ] Masked-name rendering in public lists, and the "under review" / "abgelehnt" state in your own
      view. **Half done:** masking on the board works. `name_state` is declared in the client's own
      types and then thrown away, so a holder never sees whether their name is pending or refused.
- [X] Loading and error states. Every endpoint needs a pending and a failure state, and the failure
      copy has to be written. Done: an explicit six-state machine, with the copy written.
- [X] Offline / server-down behaviour. A trial half-committed when the network drops must not look
      completed. A network failure is its own error class, and the count is never incremented
      locally — it is re-read from the server.
- [X] Statistics empty state: grey chart, one honest line. (T105)
- [ ] Multi-accounting position stated on the statistics page. (T106) D27's position is in the
      specs and the decisions, never in the site's own voice.
- [X] Remove `demoMode`, the demo pool, the hardcoded key and the fabricated figures. (T109) A
      repo-wide grep for `demoMode` now matches only this planning document.
- [X] Delete the flag and globe sprites. (T110) The switch is text.
- [ ] Rank artefact rendering for sharing — trial count and by-chance frequency **inside** the
      image, because it travels without the page around it. (T059) Not built. The badges are static
      SVGs carrying neither number, so a shared one says nothing about how it was earned.

## H. English

Both languages ship at launch (decision 5), so none of this is deferrable. **This section is
done** — all five lines, verified 2026-07-26.

- [X] Extract translatable strings and configure per-locale builds. (T062) Around 326 messages.
- [X] German and English catalogues. (T063) `build-en-catalogue --check` exits clean on 326
      messages, all translated. One nit: the committed `messages.en.xlf` holds 325 units, one
      behind — regenerating reconciles it.
- [X] Two builds, two processes, two systemd units, two nginx upstreams. (T064, T104, D24)
- [X] `hreflang` cross-references, no automatic redirect. (T068) The switch is a real link with an
      intercepted click, and nothing redirects on browser language.
- [X] **Write** the English copy. Not a translation job: "Zirbeldrüse verkalkt",
      "Erdstrahlen-Opfer" and "Orgonit-Enjoyer" are German-internet jokes that need English
      equivalents invented, in the same voice. Written, not translated — the three landed as
      "Calcified Pineal Gland", "Ley Line Victim" and "Orgonite Hat".

## I. Legal and compliance

- [X] Impressum — **done** for `.de`, from the operator's existing one. (T073)
- [ ] Verify the Impressum is complete for a site that is not purely private. Accounts, a
      leaderboard and a public dataset may make § 5 DDG apply fully. Worth ten minutes of a
      lawyer's time.
- [ ] **DSA notice-and-action.** A public board carrying user-chosen names is user-provided
      information disseminated publicly, which makes this a hosting service. Article 16 wants an
      easy-to-access electronic reporting mechanism and Articles 11-12 a published contact point.
      The Impressum contact is the mechanism; name it explicitly in the Impressum and the footer.
      No report button in v1. (T107)
- [ ] Written operator procedure for a reported name — who looks, how fast, what the action is.
      Three lines is enough, but they have to exist.
- [X] Data protection notice for both domains. (T071) **Written**, in both locales, structured as
      Article 13 GDPR with the Article 16 DSA line at the bottom. The "not written" this line
      carried was out of date.
- [X] Storage notice: the access token is stored client-side. If it is strictly necessary for a
      service the user requested, no consent banner is needed — but that reasoning belongs written
      in the privacy notice, not assumed. It is written there.
- [X] Server log retention policy, stated in the privacy notice. Stated as seven days, and the box
      actually keeps to it: `/etc/logrotate.d/nginx` is daily with `rotate 7`. What is missing is
      that nothing in `deploy/` ships that, so a fresh host would silently keep more — see T115.
- [X] Erasure: explain what survives it and why — the opaque id and the trial history, because
      deleting those would delete everyone else's verifiability too. In the notice.
- [X] **No age restriction and no age statement.** Deliberate (decision 14). Recorded here so it
      reads as a decision rather than an omission.
- [X] Confirm the AGPL notice is reachable from the running site — it is, in footer and Impressum,
      and the repository it points at is public.

## J. Operations

- [X] Register / confirm both domains, point DNS at the host. **Done** — both names resolve to the
      host through Cloudflare, and the forwarded client address is taken from `CF-Connecting-IP`.
- [X] **One machine.** Two processes sharing a SQLite file rules out splitting the domains across
      hosts; SQLite locking is unreliable over network filesystems. (D24) **Done** — both instances
      run on one host against one file.
- [X] TLS. **Done** on the deployment: Let's Encrypt per name, webroot renewal, plus a deploy hook
      that reloads nginx (webroot renewals otherwise leave the old certificate being served until it
      expires). `deploy/nginx.conf` still ships with the `ssl_certificate` lines commented out,
      because the paths are per deployment.
- [X] Rewrite `deploy/nginx.conf`: two upstreams, two `server` blocks, and delete the claim that
      `Host` selects the language build — it no longer does. (T104) **Done**, together with
      `deploy/vriltrainer@.service` as a systemd template and the README deploy section.
- [X] Install the template unit and enable both instances. **Done**, though not at the paths in
      `deploy/`: the deployment runs as an existing user out of a home directory rather than under a
      dedicated `vriltrainer` user in `/srv`, and its instance names are `de` and `com` after the
      domains, with the locale coming from the instance's env file instead of from `%i`. The unit in
      `deploy/` remains the generic form.
- [X] `token.key` — 64 hex characters, mode 0600, the same file for both instances, passed with
      `--token-key`. **If this is lost, every outstanding trial token is void.** Rotation is still
      undesigned; see the next line.
- [X] One env file per instance, each holding `LOCALE=` and `LISTEN=`. The same file the hand-start
      script reads, so a hand-start and the service cannot drift apart.
- [ ] Token key rotation procedure. Not designed. Decide now whether tokens carry a key id.
- [X] Backups. The SQLite file *is* the public audit log; losing it retroactively removes the
      verifiability of every past trial. **Live since 2026-07-26, hourly under
      `vriltrainer-backup.timer`.** The archive is a gzipped JSON export of the log rather than a
      copy of the database file, so it survives a schema move.
  - [X] Automated dump to S3. **Private**, the operator's own backup — publicly say only that
        backups run, and never promise a mirror. Uploaded with `curl --aws-sigv4`, no extra tool.
  - [X] **Verify the restore into a scratch database**, and walk the chain on the restored copy.
        An untested backup of an audit log is an untested product promise. (T079) Every run does
        this before it uploads, and compares a checksum against the snapshot so the tables the
        chain does not cover — accounts, stats, pool — are covered too.
  - [X] Confirm the bucket is not public-read — a dump carries `s_server` for trials in flight,
        which are live answers. (T080) Anonymous requests answer 403 on the bucket root and on a
        known object. The uploading identity can only put: no read, no list, no delete, so a key
        taken off the box can add archives but neither read nor destroy them.
- [ ] Uptime monitoring against the health endpoint, alerting somewhere that reaches a phone.
- [ ] Disk-space alert. The log grows forever and a full disk corrupts SQLite.
- [ ] Error reporting. `tracing` to a file will not tell you about a 500 loop at 3 a.m.
- [X] Deploy procedure written down and rehearsed. **Done** — it has been run end to end more than
      once: build, copy the binary and both language bundles, copy the manifest and every published
      `v<N>.json` beside it, restart both instances. What is still missing is a rollback that has
      been rehearsed, not just a deploy.
- [ ] Log rotation for nginx, with a SHORT retention. The access log is the only place a visitor's
      address is written down — the application itself keeps none — so it is the one file with a
      GDPR question attached. State the retention in the privacy notice. (T115, D28)
      **Half done:** the retention is stated (seven days) and this box happens to honour it, but
      only because the distribution's own logrotate is daily with `rotate 7`. Nothing in `deploy/`
      ships it, so the promise rests on a default nobody chose.
- [ ] **Auto-renew on the domains and the VPS card, multi-year where possible.** The likeliest way
      this project ends is not a bus, it is a lapsed domain. Five minutes, largest expected payoff
      in this section.
- [ ] **Registrar login to a trusted person** (decision 16). Not the server — the registrar. A
      lapsed `vriltrainer.de` does not merely go dark, it becomes available to someone else.

## K. Quality gates

- [ ] **Make the client tests run.** Karma cannot launch a browser here, there is no
      `karma.conf.js`, and the suite has never executed — the spec files only type-check. Either
      fix karma with an explicit `ChromeHeadlessNoSandbox` launcher, or move to Vitest, which
      sidesteps the browser entirely for the pure logic (`display-name`, `derive`, `framing`).
      **Recommend Vitest.**
- [ ] CI: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `npm run conformance`,
      `npm run build`, client tests. The conformance check is the one that must never go red.
- [ ] ESLint and Prettier for the client. (T007)
- [ ] Accessibility pass. Keyboard path through the whole trial loop, focus order in
      the login dialog and the name gate, screen-reader text for the histogram, contrast on the
      yellow-on-cream combinations, `prefers-reduced-motion` (already respected).
      **Partly there without a pass ever being run:** both histograms have `role="img"` and a
      label, the name field has `aria-invalid` and `aria-describedby`, the progress bar is a real
      progressbar, and reduced motion is honoured. Missing: any `aria-live`, so the verdict is
      never announced; any programmatic focus, so neither dialog nor the gate has a focus order; a
      skip link; and a recorded contrast check.
- [ ] Mobile pass on a real device. Only checked at 390 px in Chromium.
- [ ] Cross-browser: Safari and Firefox. Nothing has been looked at outside Chromium.
- [ ] Performance budget on the initial bundle — ~76 kB transferred today; keep it there once i18n
      and the real API land. **A budget exists but is not this one:** the configured warning is
      500 kB, roughly seven times the figure this line means to hold.
- [ ] Security review of the token design, the handoff codes, the rate limits and **the public
      admin endpoint**, ideally by someone who did not write them.
- [ ] Load test. 500 concurrent trials against SQLite with two writer processes is the obvious
      place this falls over. Find out before launch.
- [ ] Simulated population including an adversarial farmer. (T082)
- [ ] Cold arrival to first completed trial under 30 s, measured through the combined
      landing-and-name screen. (T091, SC-001)

## L. Content

- [X] The eight demo sprites are placeholders and must not ship as trial images. They are gone;
      trial images resolve to the real 500-image pool.
- [ ] Landing copy for the combined screen: a coordinate, eight pictures, one guess; 12,5 % is one
      in eight; the site exists to show you that. The first clause is written; the 12,5 % line and
      the framing exist only on the statistics page, which a cold visitor has not reached. Blocked
      behind T095 like the rest of that screen.
- [ ] Favicon and social preview images. **Favicon done** — a hand-drawn saucer in SVG with an
      `.ico` beside it, not the Angular default this line claims. **Social preview still missing:**
      there is no card image at all.
- [ ] `<meta>` description and Open Graph tags for both domains. The description is there and is
      rewritten per locale at bootstrap. Open Graph and Twitter tags are absent entirely, so a link
      to either domain unfurls as nothing.
- [ ] `robots.txt` and a sitemap. Neither is in the repository. Careful reading the live site here:
      `https://vriltrainer.de/robots.txt` answers 200, but that is Cloudflare's managed
      content-signal file, not ours — we have published no crawl policy of our own.

---

## 4. Launch day

The site went live before this list was walked, so treat it as the rehearsal it should have been
rather than a gate that was passed. Three lines below are confirmed; the rest are still owed.

- [X] `cargo test`, `npm run conformance`, `npm run build` all green. Re-run 2026-07-26: 289 tests
      pass, the conformance check reports ALL 7 CASES AGREE, and both bundles build.
- [X] Pool manifest hash recorded and matching what both processes serve. `sha256:94fb9006…` — the
      repository copy and the deployed copy carry the same hash and the same 500 ids, and
      `GET /api/pool/1/manifest` returns it.
- [ ] Restore a backup into a scratch database, walk the chain, re-verify a trial from it.
- [ ] TLS valid on both names; `certbot renew --dry-run` passes.
- [ ] Forwarded client address arriving correctly — it fails silently.
- [ ] Both processes up, each serving its own language, sharing one database. Create an account on
      one, switch via handoff, confirm it is the same account.
- [ ] Approve a name through the admin API; confirm it appears on the board in clear text and was
      masked before that, while its owner saw it throughout.
- [ ] Rotate the admin key and confirm the old one stops working without a restart.
- [ ] Play ten trials, unlock statistics, verify a proof, download the log.
- [ ] Remove a name; confirm the record still verifies and the account still plays.
- [X] Confirm the S3 bucket is not public-read. 403 anonymously, on the root and on an object.
- [X] Confirm the repository is public and the AGPL notice points at it. Both hold.
- [ ] Confirm the registrar login has reached the trusted person.

## 5. The first week

- [ ] Watch the aggregate. If it is not near 12,5 % after a few thousand trials, something is wrong
      with the code, not with the universe.
- [ ] Watch both tails. Asymmetry is a bug signal before it is a discovery.
- [ ] Watch the name queue. Every new player is masked until someone reviews them, and that
      bites hardest exactly on the days the site is growing.
- [ ] Watch log growth against disk.
- [ ] Have an answer ready for the first person claiming a real effect. They will be a false
      positive; the statistics page already explains why, but a prepared answer beats an
      improvised one.

---

## 6. Still open

Two of the four are answered. Reviewed 2026-07-26.

1. ~~**Is the repository public?**~~ **Yes** (T008). Confirmed from outside: the anonymous GitHub
   API returns `"private": false` and `"license": "AGPL-3.0"`.
2. ~~**Where are the pool images served from?**~~ **Compiled into the binary** (D29), served by the
   application. Not an nginx directory, not the database, not a CDN. The cost is that a pool change
   means a rebuild and a version cut, which is now the documented deploy rule.
3. **Token key rotation**: do trial tokens carry a key id? Still undesigned, and now more urgent
   than it was — the key is set and in use, so any answer that is not "carry a key id from the
   start" costs an outage of every token in flight.
4. **Which host**, and is the VPS card set to auto-renew? The host is settled in practice; the
   auto-renew question is untouched, as is the registrar handover in § J.
