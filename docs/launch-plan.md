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

**Done and load-bearing:**

- The derivation contract. `shared/vectors/README.md` is normative, `shared/vectors/derivation.json`
  holds the fixtures, and Rust and TypeScript agree on all seven cases. This is the hardest thing
  in the project and it is finished.
- Rust: `framing`, `pool`, `trial/{commit,derive,token}`, `log/chain`, `bin/gen_vectors`. About 900
  lines that compile and are tested.
- The client interface, complete as a demo: trial loop, statistics, leaderboard, Impressum, the
  full in-browser derivation proof, eleven illustrated ranks, the animated horizon, the name gate.

**Not started, at all:**

- `server/src/main.rs` is three lines and prints "not yet wired up". There is **no HTTP layer, no
  database, no axum, no rusqlite** — `server/Cargo.toml` does not list them, and T003 claimed
  otherwise until it was unticked. Every endpoint in `contracts/http-api.md` is unimplemented.
- `tools/poolctl/src/main.rs` is `fn main() { println!("probe"); }`. One line.
- `shared/pool/` is an empty directory. There is no image pool.
- No English build. No i18n extraction. The client is German string literals.
- The client talks to nothing: `ApiService.demoMode = true` and every value is generated locally.

**Unknown:** whether the GitHub repository is public yet (T008). `gh` is not authenticated on this
machine, so this could not be checked. Until it is public, "open source" is an intention.

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
| 11 | Reporting via the **Impressum contact**. Names are **pre-approved**; `<anonymous>` until then. | D25, T107 |
| 12-13 | Admin API is **public**, one key, hash in the database, `--rotate` with no restart, **reversible operations only**. | D25, T098-T099 |
| 14 | **No age restriction** of any kind. Deliberate, not an oversight. | this document |
| 15 | **No flags** on the language switch — DE and EN as text. | T110 |
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

Cutting the pool from 500 to 160 took roughly three weeks off the manual work — and shipping both
languages put two back, with the name-approval machinery costing most of the rest. **Still 7–10
weeks of evenings**, spent differently.

The pool is no longer the critical path; the server is. Start the pool anyway, because it is the
only item that cannot be accelerated later by concentrating effort.

---

## A. Server foundation

- [ ] Real dependencies in `server/Cargo.toml`: axum, tokio, rusqlite, tower-http, tracing,
      tracing-subscriber, clap. (T003, unticked — it was never done)
- [ ] `server/src/db/schema.sql` — `account`, `log_entry`, `pool_version`, `pool_image`,
      `handoff_code`, `account_stats`, plus `admin_key` and the name state from § F. The log
      references the opaque account id, never the name (FR-026). (T017)
- [ ] `server/src/db/mod.rs` — WAL, reader pool, and the **two-writer discipline**: `BEGIN
      IMMEDIATE` before reading the chain head, `busy_timeout`, `UNIQUE` on sequence number and
      `prev_hash`, chain walk at startup. (T018, T103, R9, D24)
- [ ] Migration runner. Retrofitting a version table onto a live audit log is miserable.
- [ ] CLI: `--db`, `--pool`, `--listen`, and `--locale de|en`. (D24)
- [ ] `server/src/http/mod.rs` — axum router skeleton. (T022)
- [ ] Locale fixed at startup, not from `Host`. (T023, T064)
- [ ] Forwarded client address, trusted **only** from the proxy address. Without this the
      per-address limit is inert or global; done naively, any client forges its own. (T024)
- [ ] Structured logs with a request id, and an assertion that no URL fragment reaches them. (T025)
- [ ] Graceful shutdown, so a deploy does not truncate a write to the audit log.
- [ ] Health endpoint for the monitor in § J.

## B. The trial loop

- [ ] `POST /api/account` — CSPRNG token, >=128 bits, only its hash stored. (T026, T027)
- [ ] `POST /api/trial` — draw coordinate and `s_server`, write `COMMIT` **before responding**.
      (T028)
- [ ] `POST /api/trial/reveal` — combine `s_client`, derive, issue token 2. (T029)
- [ ] `POST /api/trial/answer` — evaluate, append `RESOLVE`, return `s_server`, `s_client`, nonce.
      (T030)
- [ ] Minimum viewing time, checked **before** the chosen image is examined, or the rule becomes an
      oracle for the target. (T031)
- [ ] One evaluated answer per trial; a speed-rejected answer does not consume it. (T032)
- [ ] Concurrent-uncompleted-trial cap — this is what bounds log growth. (T076)
- [ ] Per-address limit on account creation. (T075)
- [ ] Contract test for the trial endpoints. (T036)
- [ ] Test that no response before the answer identifies the target. (T037)
- [ ] Test the draw is unbiased across categories with uneven category sizes. (T094)

## C. Statistics and the leaderboard

- [ ] `account_stats` maintained incrementally on resolve. (T038)
- [ ] Wilson lower bound and deviation from chance, server side. (T039)
- [ ] **Exact** binomial tail for the per-10,000 phrasing. The client uses Abramowitz & Stegun
      7.1.26, which is fine for a live figure and not for the published one; small `n` is exactly
      where the claim is loudest. (T040)
- [ ] Block-wise advancement in blocks of 25. (T041)
- [ ] `GET /api/stats/me` — gate on completed count alone, never on success; always report the
      abandoned count. (T042)
- [ ] `GET /api/stats/aggregate` including both tail counts. (T043)
- [ ] Eligibility: the configured floor across >=3 distinct UTC days. (T055)
- [ ] **Share-based** rank assignment, band awarded at `share x eligible >= 1`, no rounding up.
      (T056, D23)
- [ ] **Background recomputation every ~15 min**, materialised, with a last-computed timestamp on
      the board. (T102)
- [ ] Thresholds as configuration, reported in the responses that depend on them. (T101, D26)
- [ ] `GET /api/leaderboard`, sorted by the Wilson lower bound. (T057)
- [ ] Pagination on `/api/leaderboard`. The client pages at 20; the contract does not mention
      paging. Add it to `contracts/http-api.md` and implement it.
- [ ] Test that the statistics gate ignores success. (T046)
- [ ] Contract tests for statistics and leaderboard. (T085, T086)
- [ ] Adversarial farmer in the simulated population. (T082, D27)

## D. The public log and verification

- [ ] `server/src/log/export.rs` — NDJSON from a sequence number, abandoned trials included as
      commits without resolves. (T052)
- [ ] `s_client` in the resolve entry, or only the participant can re-derive the decoys and SC-002
      is false. (T061)
- [ ] `GET /api/log` and `GET /api/log/head`. (T053)
- [ ] `GET /api/pool/{version}/manifest`, answering for **every** version for as long as the
      service runs. (T054, D5)
- [ ] Log format test: chain links, commitments match, abandoned trials are commits without
      resolves, aggregate recomputes from the file alone. (T060)
- [ ] Make the export prominent and invite readers to keep a copy. (T108)
- [ ] ~~Standalone verifier.~~ **Not in v1** (decision 8). SC-002 has been reworded to promise the
      published procedure rather than a shipped tool, and the interface must not claim third-party
      independence it does not have.

## E. The image pool and poolctl

- [ ] `tools/poolctl/Cargo.toml` dependencies: image, sha2, serde, clap. (T004)
- [ ] `normalise.rs` — fixed edge length, uniform requantisation, metadata stripped, id from the
      hash of the normalised bytes. The anti-*sensory*-leakage part of D5 is sound and matters at
      any pool size. (T013)
- [ ] `annotate.rs` — source URL, licence, attribution per image. (T014)
- [ ] `manifest.rs` — sorted `(id, category)` pairs, hash over them, category inside the hash. The
      ordering is normative: it silently determines every future derivation. (T015)
- [ ] `check.rs` — refuse missing source, licence or category; refuse duplicate hashes; report
      images per category so a thin category is visible before it repeats. (T093)
- [ ] Manifest format test, including that a reordered manifest is rejected. (T084)
- [ ] **Curate 160 images**, 20 categories x 8. (T016) The trap is variety *within* a category:
      D22 stops two images of the same kind sharing a trial, but twenty near-identical landscapes
      still make repetitive trials and no code repairs that.
- [ ] Image withdrawal path: a withdrawn image serves a placeholder; the manifest and the id stay,
      so every past trial remains verifiable. (D5)
- [ ] Decide the serving path — the systemd unit passes `--pool` a JSON file, so today the answer
      is "nginx serves a directory". Confirm and write it down.
- [ ] Attribution page listing every image, source and licence. CC-BY requires credit and a JSON
      manifest is not a human-readable one.

## F. Accounts, names and moderation

- [ ] Name state machine: `pending` -> `approved` | `rejected`. `<anonymous>` plus the public id on
      every public surface until approved; the holder sees their own name marked under review.
      (T096, FR-047, D25)
- [ ] Port `checkDisplayName` to the server and enforce it on submission. The client copy is UX
      only and says so at the top of the module. (T097)
- [ ] Rename, rate-limited. A rejected name does not consume the limit; the last approved name
      stays displayed until a replacement clears. (T100, FR-048)
- [ ] Erasure: permanent, account plays on under the opaque id, no new name. (FR-035)
- [ ] Rejected names discarded, not retained.
- [ ] Test that erasure leaves the record verifiable. (T074)
- [ ] Public admin API: list pending, approve, reject — **reversible only**. Everything
      destructive stays CLI behind SSH. (T098, D25)
- [ ] One admin key, hash in the database, `server admin-key --rotate`, no restart. (T099)
- [ ] Rate limit the admin endpoint, constant-time key comparison, excluded from the nginx cache
      rule, and named explicitly in the § K security review — it is the only privileged surface in
      the system.
- [ ] Disclosure at name entry that name and history are public. (T072) The gate does this; keep
      it, and add that the name appears after review.
- [ ] Handoff codes for the language switch, single use, ~30 s. (T065-T067)
- [ ] Test that a language switch preserves the account and creates no duplicate. (T090)
- [ ] Backlog, not v1: re-run the filter over existing names when the blocklist is extended.
      Otherwise it only ever applies to accounts created after the last edit.
- [ ] **Not needed:** name uniqueness. The public identifier distinguishes accounts (FR-049).
      Accepted consequence: two accounts can both be `otherfren`, so impersonation is possible and
      the id is the only defence — which is why FR-029 forces it to be shown alongside.

## G. Client: replace the demo with the real thing

- [ ] `/` becomes the combined landing-and-name screen. (T095)
- [ ] `ApiService` currently generates `s_server`, the nonce and the coordinate locally and runs
      `derive()` against an eight-image demo pool. Replace with real HTTP calls. **Not a flag
      flip** — the flow differs, and the proof panel must be re-pointed at server-returned values.
- [ ] Real access-link handling: read the fragment, store it, clear the address bar with
      `history.replaceState`. (T034) The key is a hardcoded constant today.
- [ ] Persist the account locally so a reload does not lose it.
- [ ] Re-prompt to save the access link at the statistics threshold. (T089)
- [ ] `<anonymous>` rendering and the "under review" state for your own name.
- [ ] Loading and error states. There are none — the demo is synchronous and cannot fail. Every
      endpoint needs a pending and a failure state, and the failure copy has to be written.
- [ ] Offline / server-down behaviour. A trial half-committed when the network drops must not look
      completed.
- [ ] Statistics empty state: grey chart, one honest line. (T105)
- [ ] Multi-accounting position stated on the statistics page. (T106)
- [ ] Remove `demoMode`, the demo pool, the hardcoded key and the fabricated figures. (T109)
- [ ] Delete the flag and globe sprites. (T110)
- [ ] Rank artefact rendering for sharing — trial count and by-chance frequency **inside** the
      image, because it travels without the page around it. (T059) Not built.

## H. English

Both languages ship at launch (decision 5), so none of this is deferrable.

- [ ] Extract translatable strings and configure per-locale builds. (T062) Nothing is extracted.
- [ ] German and English catalogues. (T063)
- [ ] Two builds, two processes, two systemd units, two nginx upstreams. (T064, T104, D24)
- [ ] `hreflang` cross-references, no automatic redirect. (T068) `index.html` has the links.
- [ ] **Write** the English copy. Not a translation job: "Zirbeldrüse verkalkt",
      "Erdstrahlen-Opfer" and "Orgonit-Enjoyer" are German-internet jokes that need English
      equivalents invented, in the same voice. Budget for writing the English ladder.

## I. Legal and compliance

- [ ] Impressum — **done** for `.de`, from the operator's existing one. (T073)
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
- [ ] Data protection notice for both domains. (T071) **Not written.** The GDPR follows the
      operator and the visitor, not the interface language.
- [ ] Storage notice: the access token is stored client-side. If it is strictly necessary for a
      service the user requested, no consent banner is needed — but that reasoning belongs written
      in the privacy notice, not assumed.
- [ ] Server log retention policy, stated in the privacy notice. nginx logs IP addresses.
- [ ] Erasure: explain what survives it and why — the opaque id and the trial history, because
      deleting those would delete everyone else's verifiability too.
- [ ] **No age restriction and no age statement.** Deliberate (decision 14). Recorded here so it
      reads as a decision rather than an omission.
- [ ] Confirm the AGPL notice is reachable from the running site — it is, in footer and Impressum.

## J. Operations

- [ ] Register / confirm both domains, point DNS at the host.
- [ ] **One machine.** Two processes sharing a SQLite file rules out splitting the domains across
      hosts; SQLite locking is unreliable over network filesystems. (D24)
- [ ] TLS. `deploy/nginx.conf` has the `ssl_certificate` lines **commented out**. Certbot for both
      names.
- [ ] Rewrite `deploy/nginx.conf`: two upstreams, two `server` blocks, and delete the claim that
      `Host` selects the language build — it no longer does. (T104)
- [ ] Create the `vriltrainer` user and `/srv/vriltrainer`; two systemd units on two ports.
- [ ] `/etc/vriltrainer/env` with `VRILTRAINER_TOKEN_KEY` — 32 random bytes, hex, mode 0600, owned
      by root. **If this is lost, every outstanding trial token is void.**
- [ ] Token key rotation procedure. Not designed. Decide now whether tokens carry a key id.
- [ ] Backups. The SQLite file *is* the public audit log; losing it retroactively removes the
      verifiability of every past trial.
  - [ ] Automated dump to S3. **Private**, the operator's own backup — publicly say only that
        backups run, and never promise a mirror.
  - [ ] **Verify the restore into a scratch database**, and walk the chain on the restored copy.
        An untested backup of an audit log is an untested product promise. (T079)
  - [ ] Confirm the bucket is not public-read — a dump carries `s_server` for trials in flight,
        which are live answers. (T080)
- [ ] Uptime monitoring against the health endpoint, alerting somewhere that reaches a phone.
- [ ] Disk-space alert. The log grows forever and a full disk corrupts SQLite.
- [ ] Error reporting. `tracing` to a file will not tell you about a 500 loop at 3 a.m.
- [ ] Deploy procedure written down and rehearsed once. The README has the scp lines; nobody has
      run them.
- [ ] Log rotation for nginx.
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
- [ ] Accessibility pass. Never done. Keyboard path through the whole trial loop, focus order in
      the login dialog and the name gate, screen-reader text for the histogram, contrast on the
      yellow-on-cream combinations, `prefers-reduced-motion` (already respected).
- [ ] Mobile pass on a real device. Only checked at 390 px in Chromium.
- [ ] Cross-browser: Safari and Firefox. Nothing has been looked at outside Chromium.
- [ ] Performance budget on the initial bundle — ~76 kB transferred today; keep it there once i18n
      and the real API land.
- [ ] Security review of the token design, the handoff codes, the rate limits and **the public
      admin endpoint**, ideally by someone who did not write them.
- [ ] Load test. 500 concurrent trials against SQLite with two writer processes is the obvious
      place this falls over. Find out before launch.
- [ ] Simulated population including an adversarial farmer. (T082)
- [ ] Cold arrival to first completed trial under 30 s, measured through the combined
      landing-and-name screen. (T091, SC-001)

## L. Content

- [ ] The eight demo sprites are placeholders and must not ship as trial images.
- [ ] Landing copy for the combined screen: a coordinate, eight pictures, one guess; 12,5 % is one
      in eight; the site exists to show you that.
- [ ] Favicon and social preview images. `favicon.ico` is the Angular default.
- [ ] `<meta>` description and Open Graph tags for both domains.
- [ ] `robots.txt` and a sitemap.

---

## 4. Launch day

- [ ] `cargo test`, `npm run conformance`, `npm run build` all green.
- [ ] Pool manifest hash recorded and matching what both processes serve.
- [ ] Restore a backup into a scratch database, walk the chain, re-verify a trial from it.
- [ ] TLS valid on both names; `certbot renew --dry-run` passes.
- [ ] Forwarded client address arriving correctly — it fails silently.
- [ ] Both processes up, each serving its own language, sharing one database. Create an account on
      one, switch via handoff, confirm it is the same account.
- [ ] Approve a name through the admin API; confirm it appears on the board and `<anonymous>`
      before that.
- [ ] Rotate the admin key and confirm the old one stops working without a restart.
- [ ] Play ten trials, unlock statistics, verify a proof, download the log.
- [ ] Remove a name; confirm the record still verifies and the account still plays.
- [ ] Confirm the S3 bucket is not public-read.
- [ ] Confirm the repository is public and the AGPL notice points at it.
- [ ] Confirm the registrar login has reached the trusted person.

## 5. The first week

- [ ] Watch the aggregate. If it is not near 12,5 % after a few thousand trials, something is wrong
      with the code, not with the universe.
- [ ] Watch both tails. Asymmetry is a bug signal before it is a discovery.
- [ ] Watch the name queue. Every new player is `<anonymous>` until someone reviews them, and that
      bites hardest exactly on the days the site is growing.
- [ ] Watch log growth against disk.
- [ ] Have an answer ready for the first person claiming a real effect. They will be a false
      positive; the statistics page already explains why, but a prepared answer beats an
      improvised one.

---

## 6. Still open

1. **Is the repository public?** (T008) Not verifiable from here.
2. **Where are the pool images served from** — nginx directory, database, or CDN? (§ E)
3. **Token key rotation**: do trial tokens carry a key id? Decide before the first key is set,
   because retrofitting one invalidates every token in flight.
4. **Which host**, and is the VPS card set to auto-renew?
