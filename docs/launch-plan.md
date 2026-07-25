# Launch plan

Everything still standing between this repository and a live `vriltrainer.de` / `vriltrainer.com`.

Written 2026-07-25. This is a working document: tick things off, and when reality disagrees with
it, change the document rather than the memory of it.

The authoritative task list remains
[`specs/001-remote-viewing-trainer/tasks.md`](../specs/001-remote-viewing-trainer/tasks.md) (76 of
94 tasks open). This plan is broader — it covers what tasks.md never had a task for: hosting, DNS,
TLS, moderation, the design work, spec drift, and the day itself.

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
  database, no axum, no rusqlite** — `server/Cargo.toml` does not even list them. Every endpoint in
  `contracts/http-api.md` is unimplemented.
- `tools/poolctl/src/main.rs` is `fn main() { println!("probe"); }`. One line.
- `shared/pool/` is an empty directory. There is no image pool.
- No English build. No i18n extraction. The client is German string literals.
- The client talks to nothing: `ApiService.demoMode = true` and every value is generated locally.

**Unknown:** whether the GitHub repository is public yet (T008). `gh` is not authenticated on this
machine, so this could not be checked. Until it is public, "open source" is an intention.

---

## 2. The two things that serialise everything

Nothing about the ordering below matters as much as these two, and they are independent of each
other, so they should run in parallel from day one.

### 2.1 The image pool (T016)

500+ freely licensed images, normalised, categorised, licence and attribution recorded for each.
This is the largest piece of manual work in the project and **it blocks every playable trial**.
It cannot be compressed by writing more code, and it cannot be started until `poolctl` exists
(§ E), which is perhaps two days of work.

The trap is variety *within* a category. D22 stops two images of the same kind sharing a trial,
but twenty near-identical landscapes still make repetitive trials, and no code repairs that.

**Estimate: 3–6 weeks of evenings, and it should start first.**

### 2.2 The server (§ A–D)

Roughly everything in `contracts/http-api.md`. The client is finished and waiting; the server is
a stub. This is the bulk of the remaining engineering.

**Estimate: 3–5 weeks of focused work.**

---

## A. Server foundation

- [ ] Add the real dependencies to `server/Cargo.toml`: axum, tokio, rusqlite, tower-http, tracing,
      tracing-subscriber, clap. **T003 is ticked in tasks.md but is not actually done** — correct it.
- [ ] `server/src/db/schema.sql` — `account`, `log_entry`, `pool_version`, `pool_image`,
      `handoff_code`, `account_stats` per data-model.md. The log references the opaque account id,
      never the name (FR-026). (T017)
- [ ] `server/src/db/mod.rs` — SQLite in WAL, one writer connection, a reader pool. (T018)
- [ ] Migration runner. Even a single-file schema needs a version table on day one; retrofitting
      one onto a live audit log is miserable.
- [ ] CLI argument parsing to match the systemd unit: `--db`, `--pool`, `--listen`.
- [ ] `server/src/http/mod.rs` — axum router skeleton. (T022)
- [ ] `server/src/http/locale.rs` — `Host`-based locale selection, the only signal D10 uses. (T023)
- [ ] `server/src/http/client_addr.rs` — trust the forwarded address **only** from the proxy
      address. Without this the per-address limit is inert or global; done naively, any client
      forges its own address. (T024)
- [ ] `server/src/http/trace.rs` — structured logs with a request id, and an assertion that no URL
      fragment can reach them. (T025)
- [ ] Graceful shutdown, so a deploy does not truncate a write to the audit log.
- [ ] Health endpoint for the monitor in § J.

## B. The trial loop

- [ ] `POST /api/account` — token from a CSPRNG, ≥128 bits, only its hash stored. (T026, T027)
- [ ] `POST /api/trial` — draw coordinate and `s_server`, write `COMMIT` **before responding**.
      (T028)
- [ ] `POST /api/trial/reveal` — combine `s_client`, derive, issue token 2 with reveal time and
      expiry. (T029)
- [ ] `POST /api/trial/answer` — evaluate, append `RESOLVE`, return `s_server`, `s_client`, nonce.
      (T030)
- [ ] Minimum viewing time, checked **before** the chosen image is examined. Anything else turns
      the rule into an oracle for the target. (T031)
- [ ] One evaluated answer per trial; a speed-rejected answer does not consume it. (T032)
- [ ] Concurrent-uncompleted-trial cap per account — this is what bounds log growth. (T076)
- [ ] Per-address limit on account creation. (T075)
- [ ] Contract test for the trial endpoints. (T036)
- [ ] Test that no response before the answer identifies the target. (T037)
- [ ] Test the draw is unbiased across categories with deliberately uneven category sizes. (T094)

## C. Statistics and the leaderboard

- [ ] `account_stats` maintained incrementally on resolve. (T038)
- [ ] Wilson lower bound and deviation from chance, server side. (T039)
- [ ] **Exact** binomial tail for the per-10,000 phrasing — not the normal approximation. The
      client currently uses Abramowitz & Stegun 7.1.26, which is fine for a live figure but not for
      the published one; small `n` is exactly where the claim is loudest. (T040)
- [ ] Block-wise advancement in blocks of 25. (T041)
- [ ] `GET /api/stats/me` — gate on completed count alone, never on success; always report the
      abandoned count. (T042)
- [ ] `GET /api/stats/aggregate` including both tail counts. (T043)
- [ ] Leaderboard eligibility: 100 completed trials across ≥3 distinct UTC days. (T055)
- [ ] Rank assignment — **see § K.1, this has changed** and D19 no longer describes what is built.
      (T056)
- [ ] `GET /api/leaderboard`, sorted by the Wilson lower bound. (T057)
- [ ] Pagination on `/api/leaderboard`. The client now pages at 20; the contract does not mention
      paging at all. Add `?page=` / `?after=` to `contracts/http-api.md` and implement it.
- [ ] Test that the statistics gate ignores success: ten trials with zero hits and ten with hits
      must behave identically. (T046)
- [ ] Contract tests for statistics and leaderboard. (T085, T086)

## D. The public log and verification

- [ ] `server/src/log/export.rs` — newline-delimited JSON from a sequence number, abandoned trials
      included as commits without resolves. (T052)
- [ ] `s_client` in the resolve entry, or only the participant can re-derive the decoys and SC-002
      is false. (T061)
- [ ] `GET /api/log` and `GET /api/log/head`. (T053)
- [ ] `GET /api/pool/{version}/manifest` — without it nobody can recompute anything. (T054)
- [ ] Log format test: chain links, commitments match, abandoned trials are commits without
      resolves, and the aggregate recomputes from the file alone. (T060)
- [ ] A standalone verifier script (Python or Rust, no npm) that takes the exported log plus the
      manifest and re-derives every trial. This is not in tasks.md and it should be: it is the
      artefact that makes "verifiable by an independent party" a fact rather than a claim.

## E. The image pool and poolctl

- [ ] `tools/poolctl/Cargo.toml` dependencies: image, sha2, serde, clap. (T004)
- [ ] `normalise.rs` — fixed edge length, uniform requantisation, metadata stripped, id from the
      hash of the normalised bytes. (T013)
- [ ] `annotate.rs` — source URL, licence, attribution per image. This is the curation interface,
      not merely a build step. (T014)
- [ ] `manifest.rs` — sorted `(id, category)` pairs, hash over them, category inside the hash.
      The ordering is normative: it silently determines every future derivation. (T015)
- [ ] `check.rs` — refuse an image with no source, no licence or no category; refuse a duplicate
      hash; report images per category so a thin category is visible before it starts repeating.
      (T093)
- [ ] Manifest format test, including that a reordered manifest is rejected. (T084)
- [ ] **Curate 500+ images.** (T016) See § 2.1.
- [ ] Decide and record the image hosting path: are normalised images served from the same nginx,
      from the SQLite blob store, or from a CDN? The systemd unit passes `--pool` a JSON file, so
      today the answer is "nginx serves a directory". Confirm and write it down.
- [ ] Licence attribution page listing every image, its source and its licence. Several free
      licences (CC-BY) require attribution and a JSON manifest is not a human-readable credit.

## F. Accounts, names and moderation

- [ ] **Port `checkDisplayName` to the server and enforce it in `POST /api/account`.** The client
      copy is UX only; it says so at the top of the module. This is not optional.
- [ ] Decide whether the name is mandatory. The client now gates play behind it (§ K.2); the spec
      does not say it must be. Whatever is decided, both ends must agree.
- [ ] Name uniqueness. The client cannot check it and the current design does not mention it. Two
      players called `otherfren` on one board is a bug people will report as cheating.
- [ ] Disclosure at name entry that the name and the full trial history are public. (T072) — the
      gate does this already; keep it when the gate becomes real.
- [ ] `DELETE /api/account/name`, self-service, authenticated by the access link. (T069, T070)
- [ ] Test that erasure leaves the record verifiable. (T074)
- [ ] Handoff codes for the language switch, single use, ~30 seconds. (T065, T066, T067)
- [ ] Test that a language switch preserves the account and creates no duplicate. (T090)
- [ ] A reporting path. A public board with free-text names needs a way to say "that one is a slur
      the filter missed" and an operator procedure for acting on it. Nothing in the spec covers
      this and a German-hosted public board without it is a liability.
- [ ] Re-check existing names when the blocklist is extended. Otherwise the filter only ever
      applies to people who signed up after the last edit.

## G. Client: replace the demo with the real thing

- [ ] `ApiService` currently generates `s_server`, the nonce and the coordinate locally and runs
      `derive()` in the browser against an eight-image demo pool. Replace with real HTTP calls.
      **This is not a flag flip** — the shapes are close but the flow is genuinely different, and
      the proof panel has to be re-pointed at server-returned values.
- [ ] Real access-link handling: read the fragment, store it, clear the address bar with
      `history.replaceState`. (T034) Currently the key is a hardcoded constant.
- [ ] Persist the account locally (the fragment token) so a reload does not lose it.
- [ ] Re-prompt to save the access link at the statistics threshold. (T089)
- [ ] Loading and error states. There are none: the demo is synchronous and cannot fail. Every
      endpoint needs a pending state and a failure state, and the failure states have to be
      written, not defaulted.
- [ ] Offline / server-down behaviour. A trial half-committed when the network drops must not look
      like a completed one.
- [ ] Remove `demoMode` and the demo pool, or keep it behind a build flag for screenshots.
- [ ] Rank artefact rendering for sharing — trial count and by-chance frequency **inside** the
      image, because it travels without the page around it. (T059) Not built.

## H. English

- [ ] Extract translatable strings and configure per-locale builds. (T062) Nothing is extracted;
      every string is a German literal in a template.
- [ ] German and English message catalogues. (T063)
- [ ] Serve the locale bundle by `Host`, one binary for both domains. (T064)
- [ ] `hreflang` cross-references, and no automatic language redirect. (T068) — `index.html` has
      the hreflang links already.
- [ ] Translate the copy. This is a real writing job, not a mechanical one: the tone is the
      product, and "Zirbeldrüse verkalkt" does not survive a literal translation. Budget for
      writing the English ladder, not translating it.
- [ ] Decide what the English rank names are. Several are German-internet jokes.

## I. Legal and compliance

- [ ] Impressum — **done** for `.de`, taken from the operator's existing one. (T073)
- [ ] Verify the Impressum is complete for a site that is not purely private. If accounts,
      leaderboards and a public dataset count as a `Telemedienangebot`, § 5 DDG applies fully.
      Worth ten minutes of a lawyer's time.
- [ ] Data protection notice for both domains. (T071) **Not written.** The GDPR follows the
      operator and the visitor, not the interface language, so it belongs on `.com` too.
- [ ] Cookie / storage notice. The access token is stored client-side. If it is strictly necessary
      for a service the user requested, no consent banner is required — but that reasoning has to
      be written down in the privacy notice, not assumed.
- [ ] Server log retention policy, and confirm it in the privacy notice. nginx logs IP addresses.
- [ ] Right to erasure: name removal exists in the plan; the privacy notice must explain what
      survives it and why (the opaque id and the trial history, because deleting them would delete
      the verifiability of everyone else's trials too).
- [ ] Age. A public leaderboard with free-text names and no age gate is a question worth answering
      deliberately rather than by omission.
- [ ] Confirm the AGPL notice is reachable from the running site — it is, in the footer and the
      Impressum.

## J. Operations

- [ ] Register / confirm both domains, and point DNS at the host.
- [ ] Provision the host. The deployment is a static binary plus a SQLite file; a 2 GB VPS is
      plenty, but the disk must be sized for a log that only grows.
- [ ] TLS. `deploy/nginx.conf` has the `ssl_certificate` lines **commented out**. Certbot for both
      names, with the `.com` on the same certificate or its own.
- [ ] Create the `vriltrainer` system user and `/srv/vriltrainer`, per the systemd unit.
- [ ] `/etc/vriltrainer/env` with `VRILTRAINER_TOKEN_KEY` — 32 random bytes, hex. Mode 0600, owned
      by root. **If this key is lost, every outstanding trial token is void.**
- [ ] Key rotation procedure. Not designed. Decide now whether tokens carry a key id.
- [ ] Backups. The SQLite file *is* the public audit log; losing it does not cost data that can be
      rebuilt, it retroactively removes the verifiability of every past trial.
  - [ ] Automated dump to S3.
  - [ ] **Verify the restore path into a scratch database.** An untested backup of the audit log is
        an untested product promise. (T079)
  - [ ] Confirm the bucket is not public-read — a dump carries `s_server` for trials still in
        flight, which are live answers. (T080)
- [ ] Uptime monitoring against the health endpoint, with an alert that reaches a phone.
- [ ] Disk-space alert. The log grows forever and a full disk corrupts SQLite.
- [ ] Error reporting. `tracing` to a file is not enough to notice a 500 loop at 3 a.m.
- [ ] Deploy procedure written down and rehearsed once against a staging host. The README has the
      scp lines; nobody has run them.
- [ ] A staging environment, or at least a second domain the same binary can serve, so the first
      real deploy is not the first deploy.
- [ ] Log rotation for nginx, and confirm access logs do not record URL fragments (they cannot —
      fragments are not sent — but confirm nothing else logs the full referrer).

## K. Spec reconciliation

Today's design work moved ahead of the written specification in three places. Each needs a decision
record and a spec edit, or the specification stops being the source of truth.

### K.1 Ranks are percentile bands now, not positions

D19 and T056 describe a positional ladder — seven ranks, fixed seat counts (1–3, 4–10, …),
withheld entirely below 200 eligible accounts. What is built is **eleven ranks as shares of the
population** (best 0.1 %, 0.5 %, 2 %, 7 %, 20 %, middle 60 %, mirrored), with new names
(Annunaki, Insektoider Loosh-Farmer, Psionisches Asset, Zirbeldrüse verkalkt, Erdstrahlen-Opfer,
Orgonit-Enjoyer, Psi-Nullleiter, Kartoffel).

- [ ] Write **D23** in `docs/trial-protocol-decisions.md` recording the change and why: a share
      means the same thing at any population size, a seat does not, and a third place out of ten
      is not a title.
- [ ] Update FR-042, T056, T081 and the spec's ladder.
- [ ] Decide whether the 200-eligible-account withholding rule survives. It probably should — 0.1 %
      of 40 players is not a rank either.

### K.2 A name is now mandatory before the first trial

The gate is built and the Login control is hidden until a name exists. The spec does not require
this and FR-035 implies a name is removable, which raises a question the gate does not answer:
**what happens after somebody removes their name?** Today they would be sent back to the gate.

- [ ] Decide: mandatory at signup and removable afterwards (returning to an anonymous id), or
      mandatory always. Record it, then make the client and server agree.

### K.3 The client-side statistics unlock

The client gates personal statistics at 10 completed trials and shows a progress bar before that.
That matches FR-017's spirit but the threshold is a client constant.

- [ ] Move the threshold into the API response so it is not defined in two places.

### K.4 Other drift worth recording

- [ ] The proof panel now shows the complete preimage and all four derivation steps. That exceeds
      FR-023 and is worth writing into the spec as the standard, because it is the thing that
      makes the product's claim real.
- [ ] Eleven rank artefacts exist as original pixel work (T081 says seven). Mark it done, correct
      the count, and note that they are original — which is what closes the licensing question.
- [ ] `anyComponentStyle` budgets were raised 4→6 kB / 8→10 kB.

## L. Quality gates

- [ ] **Make `ng test` run.** Karma cannot launch a browser in this environment; there is no
      `karma.conf.js` in the repo and the suite has never executed. Three spec files now exist and
      only type-check. Either fix karma with an explicit `ChromeHeadlessNoSandbox` launcher, or
      move the client to Vitest, which the Angular CLI now supports and which would sidestep the
      browser entirely for the pure logic (`display-name`, `derive`, `framing`).
- [ ] Wire CI: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `npm run
      conformance`, `npm run build`, `ng test`. The conformance check is the one that must never go
      red. (Partially T007.)
- [ ] ESLint and Prettier for the client. (T007)
- [ ] Accessibility pass. Never done. Keyboard path through the whole trial loop, focus order in
      the login dialog and the name gate, the histogram's screen-reader text, colour contrast on
      the yellow-on-cream combinations, and `prefers-reduced-motion` (already respected).
- [ ] Mobile pass on a real device. Checked at 390 px in Chromium only.
- [ ] Cross-browser: Safari and Firefox. `<dialog>`, `@if` control flow and `image-rendering:
      pixelated` are all fine, but nothing has been looked at outside Chromium.
- [ ] Performance budget on the initial bundle. Currently ~76 kB transferred, which is good; keep
      it that way once i18n and the real API land.
- [ ] Security review of the token design, the handoff codes and the rate limits — ideally by
      someone who did not write them.
- [ ] Load test. 500 concurrent trials against SQLite with one writer connection is the obvious
      place this falls over; find out before launch, not during.
- [ ] Simulated population: random players, confirm the aggregate lands within sampling bounds of
      12.5 % and the two tails stay comparable. (T082) This is also the honest smoke test of the
      whole product thesis.
- [ ] Time from cold arrival to first completed trial under 30 seconds. (T091) The name gate makes
      this harder than it was — measure it after the gate, not before.

## M. Content and copy

- [ ] The eight demo sprites are placeholders and must not ship as trial images.
- [ ] A landing page or an "about" surface. Today `/` redirects to `/trial`, so a first-time
      visitor meets the name gate with no explanation of what the site is. That is the single
      biggest conversion problem in the current build.
- [ ] Decide what happens on the `.com` before English exists. Do not launch it half-translated.
- [ ] Favicon and social preview images. `favicon.ico` is the Angular default.
- [ ] `<meta>` description and Open Graph tags for both domains.
- [ ] `robots.txt` and a sitemap.

---

## 4. Launch day

- [ ] `cargo test`, `npm run conformance`, `npm run build` all green.
- [ ] Pool manifest hash recorded and matching what the server serves.
- [ ] Restore a backup into a scratch database and re-verify a trial from it.
- [ ] TLS certificate valid on both names, and auto-renewal proven with `certbot renew --dry-run`.
- [ ] Both headers arriving: log a request and confirm the service sees a real client address and
      an unchanged `Host`. Both fail silently.
- [ ] Create an account, play ten trials, unlock statistics, verify a proof, download the log, and
      re-derive one trial from the log with the standalone verifier.
- [ ] Remove a name and confirm the record still verifies.
- [ ] Confirm the S3 bucket is not public-read.
- [ ] Confirm the repository is public and the AGPL notice on the site points at it.

## 5. The first week

- [ ] Watch the aggregate. If it is not near 12.5 % after a few thousand trials, something is
      wrong with the code, not with the universe.
- [ ] Watch both tails. Asymmetry is a bug signal before it is a discovery.
- [ ] Watch for names the filter missed.
- [ ] Watch log growth against disk.
- [ ] Have a plan for the first person who claims a real effect. They will be a false positive and
      the statistics page already explains why — but a prepared answer beats an improvised one.

---

## 6. Critical path

```
poolctl (2d) ──> curate 500 images (3–6 weeks, manual) ─────────────┐
                                                                     ├──> integration ──> launch
server foundation (1w) ──> trial loop (1w) ──> stats + log (1.5w) ──┘         (1w)
                                                                     │
client re-wiring (1w) ───────────────────────────────────────────────┘
English (1w, parallel)  ·  ops + legal (1w, parallel)
```

**The pool is the critical path and it is manual.** Start it in week one, before the server work,
because it is the only item that cannot be accelerated later by concentrating effort. Everything
else fits inside its shadow.

Realistic: **6–10 weeks of evenings** to a `.de` launch. The `.com` should follow separately once
the English copy is written rather than translated.

A defensible smaller first launch: German only, no leaderboard (it needs 200 eligible accounts
anyway), no handoff codes. That removes § H entirely and most of § F, and it is roughly four weeks
instead of eight.

---

## 7. Decisions needed from the operator

1. **Is the name mandatory?** (§ K.2) It changes the account model and the first-run flow.
2. **What happens to `.com` at launch?** Ship both, or `.de` first?
3. **Where are the images hosted** — nginx directory, database, or CDN? (§ E)
4. **Is the leaderboard in the first launch at all?** It cannot show anything until 200 eligible
   accounts exist, which will take months.
5. **Which host, and who has the credentials** if you are unavailable? The audit log has no second
   copy of its operator.
6. **Is the eagle staying?** The black-white-red flag is gone; the DE mark is now the
   double-headed eagle of the Holy Roman Empire, black on gold. This is a milder marker than what
   it replaced — the Nazi *Reichsadler* was single-headed, and Reichsbürger iconography leans on
   1871 (black-white-red, single-headed eagle), so the *Doppeladler* points at 1400 rather than at
   1871 or 1935. It is also not specifically German: the same device carries Habsburg, Russian,
   Serbian and Albanian arms, which blunts it further.

   What has not changed is the underlying problem: it is still functional chrome rather than a
   joke rank, so it is the one element on the page not obviously in quotation marks. Decide
   whether the language switch should carry a national mark at all, or just the letters `DE` /
   `EN`.
7. **Age policy**, and whether the site says anything about it.
8. **The moderation escalation path** — who acts on a reported name, and how fast?
