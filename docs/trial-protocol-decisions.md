# Trial Protocol — Decisions

Running record of two `/grill-me` sessions held 2026-07-25 — the first on the trial mechanism
(D1–D17), the second on the product concept (D18–D21). This file is **input** for
`specs/001-remote-viewing-trainer/`; it is not itself a Spec Kit artifact and carries no
authority over the spec or the plan.

Status: **both sessions complete** — 22 decisions settled, no open questions. Remaining items
are actions only, listed at the bottom.

## What vriltrainer is

An online trainer for remote viewing. A user enters a username, is shown a coordinate,
clicks to reveal a set of images, picks one, and is told whether the pick matched the
target. Score is tracked; there is a leaderboard and a statistics view reporting whether
the user's hit rate deviates significantly from chance (z-score). Single-page Angular
application, bilingual, to be hosted on vriltrainer.com and vriltrainer.de.

## D1 — Target selection is server-side, before the choice

The target is drawn on the server and never sent to the client until after the pick has
been submitted. The client receives the image set only, shuffled.

The target is fixed **before** the user chooses, not after. This is a clairvoyance design
and matches what the product name claims. A precognition design (draw after the choice)
would also be statistically sound but asserts something different about what is measured.

Rationale: a client-side target is visible in devtools before the click. Tolerable for a
private practice mode, fatal for a public leaderboard and for any claim of significance —
the z-score would measure willingness to press F12.

## D2 — Verifiability target is level B: per-trial commitment plus public audit log

Three levels were considered:

| Level | Protects against | Decision |
|---|---|---|
| A | Server alters the target after seeing the pick | included in B |
| B | Operator draws unfairly, or deletes inconvenient trials | **chosen** |
| C | Operator rewrites the log after the fact | via anchoring, see D4 |

Level B was chosen from the start rather than deferred, because the leaderboard makes a
significance claim, and a significance claim that rests on trusting the operator is worth
little to the audience most likely to care.

## D3 — Randomness is two-party; the client contributes a nonce

Neither side controls the outcome alone:

```
1. Trial start    server: s_server ← random, nonce ← random
                  → client: coordinate, C = framed(s_server, nonce, coordinate),
                            pool_manifest_hash
2. Reveal click   client: s_client ← crypto.getRandomValues()
                  → server
                  seed   = framed(s_server, s_client)
                  8 categories, one image each, target index, order — all
                  drawn from the same stream (see D22 for the four steps)
                  → client: the N images
3. Pick           → server
4. Reveal         → client: s_server, nonce
                  client verifies framed(s_server, nonce, coordinate) == C
                  client recomputes seed, target, decoys, order — must match
```

The commitment `C` **must** travel with the coordinate in step 1. A proof produced only at
step 4 verifies a claim the server was free to invent after seeing the pick, and is
worthless.

The coordinate is **inside** the hash. This was recorded incorrectly at first — as
`H(s_server ‖ nonce)` — while the prose beneath it claimed the coordinate was bound. It was
not: with the coordinate outside the hash, the same commitment could be paired with any
coordinate after the fact, and the intended statement — *this* coordinate pointed at *this*
image — was unprovable. Corrected on 2026-07-25, before any implementation existed.

`framed(…)` length-prefixes each field: `SHA-256(LE64(|p₀|) ‖ p₀ ‖ LE64(|p₁|) ‖ p₁ ‖ …)`. Plain
concatenation is ambiguous across variable-length fields, and the fixed sizes that made it safe
here — 32-byte seeds, an `NNNN-NNNN` coordinate — are an argument that expires silently the day a
format changes. Found while implementing, fixed before anything was published.

`s_client` travels with the reveal click, so the target is determined before the user picks
— D1 is preserved.

Consequences:

- **The whole set is derived from the seed** — target, decoys and display order. If the set
  were assembled after the target index were known, it could be stacked so the target is
  distinguishable by resolution, source, or subject matter. Sensory leakage of this kind is
  the classic failure mode of forced-choice ESP experiments.
- **The image pool must be public, versioned and hashed**, or the client cannot recompute
  anything. `pool_manifest_hash` is part of the trial.
- **Trials are written to the log at commit time, not at reveal.** Otherwise the operator can
  observe `s_client`, compute the outcome, and let unwanted trials die of a "network error".
  Logging at commit makes aborts visible as gaps and countable.

## D4 — No public randomness beacon; no chain anchoring for now

Public randomness beacons (drand, NIST, blockhash, RANDAO) are unusable here: their values
are published, so any user could compute the target. The only variant that avoids this uses
a beacon round that does not yet exist at viewing time, which converts the experiment into
precognition and contradicts D1. This exclusion is permanent, not a scoping decision.

Anchoring the log into Bitcoin — via OpenTimestamps, which would have cost nothing and
required no wallet — is **deferred**. Level C is therefore not reached for now.

Consequence to be honest about in the UI: without an anchor, a published log proves only
what observers have actually seen. An operator can still rewrite history for anyone who did
not keep a copy of an earlier head. The commitment scheme in D3 is unaffected — it stands on
its own per trial — but claims in the interface must say "published" rather than
"tamper-proof".

Note if this is revisited: the project is **opentimestamps.org**. `opentimestamps.com` is
not the project — it resolves elsewhere and resets TLS connections.

Timestamping also proves less than it appears to: it establishes non-backdating, not
uniqueness. An operator can maintain two divergent logs and stamp both, and each proof
verifies. Detecting that requires publication plus clients comparing heads, in the manner of
Certificate Transparency. Anchoring alone would never have closed that gap.

## D5 — Public image pool, at least 160 images at launch

The pool is public, versioned and hashed, as D3 requires: without it the client cannot
recompute a trial and the audit story collapses. `P >= 160` at launch, growing afterwards.

**Amended 2026-07-25. The original sizing rationale was wrong and is recorded here because
someone will otherwise reinvent it.** It read: with a public log a user can look up image sets
they have seen before, so at P = 500 exact repeats never occur, while a small pool would let a
lookup table accumulate and kill the leaderboard.

That threat does not exist. The target index is drawn from `seed = framed(s_server, s_client)`,
freshly per trial, and D22 step 3 draws it uniformly over the eight shown. Recognising the exact
eight images from a past trial tells an attacker nothing about this trial's target, because this
trial's seed is not that one's. **Pool size does not appear anywhere in the target's
distribution.** A lookup table over image sets buys nothing.

What P actually controls is repetition as experience. A player sees a given image roughly `8T/P`
times over `T` trials: 5 times at P = 160 over 100 trials, 10 times at P = 80. That is a boredom
and credibility constraint, and it degrades gracefully rather than falling off a cliff. Even on
the original argument's own terms 500 was far too conservative — at 20 categories of 8 there are
already C(20,8) x 8^8 ~ 2x10^12 possible sets.

The pool is versioned (`pool_version`, `GET /api/pool/{version}/manifest`) and every trial records
the manifest hash it was drawn under, so growing the pool invalidates nothing. Launch at 160,
ship v2 at 500 while live.

**Manifests are served for every version for as long as the service runs**, because a trial
recorded under v1 stays verifiable only while v1's manifest answers. **Image bytes are
replaceable**: the derivation is computed over ids, not bytes, so a withdrawn image — a licence
that turns out to be wrong, a takedown — costs the ability to *look* at that trial and costs
nothing about checking it. That separation is what lets a takedown be honoured without touching
the log.

Pipeline requirements, all of them anti-leakage measures: fixed edge length, uniform
requantization, metadata stripped, opaque IDs derived from the normalized bytes rather than
filenames. Anything that distinguishes the target from its decoys — resolution, aspect
ratio, compression artifacts, the colour signature of a particular source — is a sensory
channel, and sensory leakage is the classic failure mode of forced-choice ESP experiments.

Manifest: sorted list of image IDs **with their category** (D22), hashed as a whole to give the
`pool_manifest_hash` carried in each trial. The category belongs inside the hash — otherwise it
could be reassigned without the manifest appearing to change, silently altering every future
derivation. Extending the pool creates a new version; older trials stay verifiable against the
version they were run under.

A Merkle root was specified here originally. It is unnecessary: a Merkle tree buys inclusion
proofs for a single element without downloading everything, and the manifest is published whole
anyway. A plain hash over the sorted list is equivalent for this purpose and simpler in two
implementations. This follows the same reasoning that chose a hash chain over a Merkle tree for
the log in D17 — correct it if a Merkle root was wanted for a reason not captured here.

Licensing: **only free / public-domain images** — confirmed. CC0 and public domain sources
such as Wikimedia Commons (PD), Unsplash, Pexels and openverse. CC-BY is avoided: the
attribution would have to be rendered in the interface, where it becomes a sensory channel
distinguishing one image from the others. Commercial-looking hosting on a `.de` domain makes
casual reuse of found images a real liability, so provenance and licence are tracked per
image in the manifest.

## D6 — Open source, at github.com/otherfren/vriltrainer

The repository is already the push target, but it is **currently private**: the unauthenticated
GitHub API reports "Not Found" while SSH pushes succeed. Making it public is an outstanding
action, not a completed one.

Open source fits the design rather than straining it. D3 never relied on the algorithm being
secret — only on `s_server` staying secret until reveal — so publishing the derivation costs
nothing and is in fact required for third parties to reimplement and check it.

Two things publication does not buy, both worth honest wording in the interface:

- **Deployed code is not the published code.** Anyone can read the source; nobody can confirm
  the running server is that source. Reproducible builds would narrow the gap, not close it.
  This is the same shape as the equivocation gap noted in D4.
- The constitution's rule against secrets in the repository stops being hygiene and becomes
  load-bearing.

Licence: undecided. Recommendation is **AGPL-3.0**, matching darkfi next door, and because a
hosted service is precisely the case AGPL exists for — a fork running a dishonest instance
would be obliged to publish its modifications.

## D7 — Rust plus SQLite on the server, TypeScript and Angular in the browser

Node remains a build-time dependency for the Angular toolchain only; it does not run in
production. Deployment is a static binary and a database file.

The derivation from D3 is therefore implemented twice: once in Rust on the server, once in
TypeScript for the in-browser verifier. This was the main argument for a TypeScript backend —
write it once, share the module — and it was rejected deliberately. Shared code that checks
itself against itself demonstrates nothing; two independent implementations agreeing on
shared test vectors are evidence that the specification is right.

The cost is real and must not be discovered later: **test vectors for the derivation are
mandatory, not optional**. Without them, divergence between the two implementations surfaces
as verification failures on honest trials, which is an expensive bug to chase.

## D8 — Eight images per trial; statistics gated on trial count, never on success

`N = 8`, so the chance rate is 12.5 %. The statistics section appears once a user has
completed **10 trials**.

Four measures against the ways a public psi-testing site manufactures false positives:

1. **Leaderboard ranks by the Wilson lower bound** of the binomial interval, not by raw hit
   rate. Small samples are penalised automatically, so a user with four trials and four hits
   does not top the table, and no arbitrary minimum-trials rule is needed.
2. **The personal z-score is shown with its context**: how many users, out of those with at
   least as many trials, would reach this value by chance alone. Without that line the number
   is not interpretable.
3. **Block-wise evaluation** — the z-score advances per completed block of trials rather than
   after every single one, which blunts optional stopping. A user who plays until the number
   looks good and then stops otherwise inflates the false-positive rate far beyond 5 %.
4. **The aggregate over all trials by all users is the scientifically load-bearing figure**
   and belongs prominently on the statistics page. It is immune to the selection effect that
   makes the top of any leaderboard look remarkable under pure chance.

An earlier form of the gate — show statistics only once the user has at least one hit — was
**dropped**. It conditions the displayed population on success. At N = 8 over 10 trials, a
user with no ability at all has a 26.3 % chance of scoring zero and vanishing from view; the
survivors then average 1.70 hits instead of 1.25, an apparent rate of 17.0 % against a true
12.5 %. The gate would have made the statistics page overstate by a third on the very number
it exists to measure honestly. Gating on trial count achieves the same goal — no statistics
on two data points — without filtering by outcome.

Rule that follows: **gate the display, never the data.** The aggregate in measure 4 runs over
every trial in the log, including those of users who never saw a statistics page.

If the hit condition is reinstated for UX reasons, the displayed personal z-score must be
computed conditioned on "at least one hit", or it is simply wrong.

## D9 — Identity is a capability URL; no recovery

The user invents a name; the server creates the account and issues a personal secret login
URL. There is no registration, no email, no password. Progress is tracked against that
account. The leaderboard shows the chosen name together with a separate public ID.

**The token lives in the URL fragment, not the path:**

```
https://vriltrainer.com/#t=<128-bit token>      not   /login/<token>
```

Fragments are never transmitted to the server — absent from requests, from access logs, and
from the `Referer` header. On load the client reads the token, stores it, and clears the
address bar via `history.replaceState`. The bookmark keeps the fragment; the visible URL does
not.

This matters more here than in most applications carrying a capability URL. The product's
whole purpose is to make people show their results — leaderboard placement, an impressive
z-score. A screenshot of that page with the address bar in frame publishes the account's
credentials. A design that encourages sharing must not put the secret where sharing captures
it.

Supporting requirements:

- Token from a CSPRNG, at least 128 bits.
- Server stores only a hash of it. It is a password and is treated as one.
- `Referrer-Policy: no-referrer`.
- Requires Angular's default `PathLocationStrategy`; `HashLocationStrategy` would collide
  with the fragment.
- The public leaderboard ID is drawn independently and is **not** derived from the token.

**Display and reminders:** the URL stays permanently visible but discreet, and is offered
again explicitly at 10 completed trials — the same threshold at which the statistics and
z-score appear under D8. That is the first moment the account is worth keeping; a warning at
first contact, before any trial has been played, is ignored by design.

**It is masked by default, behind a reveal button**, so the page can be streamed or screen-shared
without exposing the secret. This extends the same reasoning that puts the token in the
fragment: the interface must stay safe to show, because showing it is what users do here.

Two refinements that follow from the same principle: copy-to-clipboard must work **without**
revealing, so the common path never renders the secret at all, and a revealed URL should
re-mask itself after a short timeout rather than staying open for the rest of the session.

**No recovery, deliberately.** Lose the URL and the account and its history are gone. An
optional password as a second key was considered and rejected: it adds an authentication path
to maintain for a case the user was warned about twice. Email recovery was rejected more
firmly — it costs the frictionless entry, requires mail infrastructure, and pulls real GDPR
obligations onto a solo operator running two domains.

**What this does not solve:** Sybil accounts. Throwaway identities are as cheap with login
URLs as with bare names, so someone can farm many accounts and promote whichever one drifts
high by chance. That is the deliberate form of the multiple-comparisons problem in D8, and it
is answered on the display side — a leaderboard minimum in the hundreds of trials, and the
aggregate figure carrying the main claim rather than the top entry.

The public ID also only half-solves impersonation: it makes two accounts named `otherfren`
distinguishable, but a stranger cannot tell which is the original.

## D10 — Domain is the language

`vriltrainer.de` serves German, `vriltrainer.com` serves English. Two languages, two domains,
one mapping.

This permits Angular's built-in `@angular/localize`: one bundle compiled per locale,
translation resolved at build time, no runtime i18n library and no runtime cost. Switching
language is a link to the other domain. The two builds reference each other via `hreflang`,
so the domains do not compete in search results.

The rejected alternative was serving both languages from both domains with a runtime switcher
(transloco or similar). It buys instant switching without a reload and costs a runtime
dependency plus fourfold duplicate content — the same page, two languages, two domains — which
then has to be untangled with `canonical` and `hreflang` anyway.

Consequences:

- **A third language breaks the mapping.** Domain-as-language does not extend past the number
  of domains owned. Adding one means revisiting this decision, not appending to it.
- ~~The Rust server from D7 selects the locale bundle by `Host` header, so two languages remain
  one binary and one deployment.~~ **Superseded by D24**: the locale is a startup flag and each
  domain gets its own process.

**Confirmed:** the two domains are functionally identical apart from language. One backend,
one database, one leaderboard. Separate per-domain leaderboards would have split the sample
and weakened the aggregate figure that D8 makes the load-bearing claim.

## D11 — The language switch carries the session across domains

A language switch button sits top right and moves the user to the other domain.

`vriltrainer.de` and `vriltrainer.com` are **separate origins**, so the `localStorage` holding
the D9 token does not travel with the user. Without deliberate handover, clicking the switch
arrives on the other domain as an anonymous first-time visitor — progress gone, and likely a
duplicate account created, which would put one person into the leaderboard and the aggregate
twice.

The session is therefore handed over explicitly, via a **single-use handoff code**: the origin
domain mints a code valid for about 30 seconds, the button navigates to `#h=<code>` on the
target domain, which exchanges it for the real token and burns it. Both domains share a
backend, so redemption is a lookup.

The simpler alternative — putting the long-lived token straight into the target URL fragment —
was rejected. It would place the secret in the address bar and in the target domain's history,
undoing precisely the protection the reveal button in D9 exists to provide. Streaming is a
stated use case, and a language switch must not be the one action that breaks it.

No automatic redirect based on `Accept-Language` on first visit. Someone who types `.com`
wants `.com`; a discreet "also available in German" hint respects that.

## D12 — Hosting on an existing Hetzner server, backups dumped to S3

Backend and database run on the operator's existing Hetzner server. Backups are already
handled by a scheduled script that dumps the database and pushes it to S3.

Why the backup matters more than usual here: the database is not merely user data, it **is**
the public audit log from D2. Losing it does not cost records that can be rebuilt — it
retroactively makes every past trial unverifiable and voids the significance claim the product
is built on. Backup is part of the product promise, not operational hygiene.

Two notes that follow from the other decisions:

- D9 stores only a hash of each login token, so a backup carries no usable credentials. The
  audit log is public by design. The one genuinely secret thing in a dump is `s_server` for
  trials that are committed but not yet revealed, which would expose pending targets — so the
  **bucket must not be public-read**, and short trial lifetimes keep the exposure small.
- Deployment is the D7 static binary plus a database file, both domains on the one instance,
  locale bundle selected by `Host`. No container runtime is involved.

## D13 — Full public log export; names stay outside the hash chain

The audit log gets a **full public export endpoint**, serving entries from a given sequence
number onward plus the current head hash — not merely a published head. A head alone proves
nothing, because nobody can check that the entries beneath it agree with it. The export is
also free redundancy: copies pulled by third parties sit outside the operator's control and
shrink the gap left by deferring the anchor in D4.

**Data protection applies to both domains, not just the German one.** The GDPR follows the
controller's establishment and the data subject's location, never the language of the
interface. Operating from a Hetzner server under a `.de` domain means it applies to
`vriltrainer.com` in full. The notice therefore appears on both, each in its own language.
The `.de` Impressum obligation is separate from the GDPR and needs its own check — not covered
by this decision, and not legal advice.

Disclosure belongs at the moment the name is entered, not in fine print: the chosen name and
the complete trial history are public by construction, and anyone can reconstruct a user's
full run of hits and misses.

**Erasure versus an append-only log — resolved in the data model, and only resolvable there:**
Article 17 grants a right to deletion; a hash-chained log cannot drop an entry without
invalidating every proof after it. One deletion request would otherwise void verifiability
from that point on, and this is not a hypothetical for a site publishing named hit rates.

Therefore: **only the opaque account ID ever enters the hashed log. The self-chosen name lives
in a separate mutable table** and is joined in for the leaderboard and for display. Erasure
removes the name; the log stays intact and fully verifiable, and what remains is a trial
history under a random identifier with no personal reference.

This costs nothing — the same data, cut differently — but it cannot be retrofitted. Once names
are inside the chain, they cannot be taken out.

## D14 — AGPL-3.0

Chosen over a permissive licence deliberately. A hosted service is the case the AGPL exists
for: a fork running a modified, dishonest instance is obliged to publish its modifications.
For a project whose central claim is verifiability, that is a substantive alignment rather
than a preference. It also matches darkfi in the same working directory.

## D15 — Delivery order

Two items come **before** P1, because everything else waits on them:

- **The image pool.** Several hundred curated images through the normalization pipeline of
  D5. The largest piece of manual work in the project, and it cannot be automated away — only
  the pipeline can, not the selection.
- **Test vectors for the derivation** in D3. D7 puts one implementation in Rust and one in
  TypeScript; without shared vectors the second is built blind.

| Phase | Content | Why here |
|---|---|---|
| **P1 — playable** | name to trial loop (coordinate, 8 images, pick, reveal), commit-reveal from D3, capability URL from D9, pool in place | without this there is no product |
| **P2 — measurable** | score, statistics from 10 trials, z-score with its context line, aggregate figure | the distinguishing feature |
| **P3 — verifiable** | leaderboard on the Wilson lower bound, public log export, in-browser verification | level B only becomes real here |
| **P4 — public** | second language, language switch with handoff code, GDPR notice and Impressum, domains live | immediately before launch, not earlier |

Caveat carried into planning: the leaderboard sits in P3, but **without the log export it is an
unsupported assertion**. Launching earlier means launching without a leaderboard, not with an
unverifiable one.

## D16 — Trial state rides in a server-encrypted token; a minimal commit row stays

Everything bulky or secret about a trial travels to the client inside a token only the server
can decrypt, and comes back when the trial is completed. Authenticated encryption, bound to the
account and the sequence number so tokens cannot be swapped between accounts.

Two token states are needed, not one, because D3 derives the candidate set from `s_client`: at
trial start the set does not exist yet. Token 1 accompanies the coordinate and carries
`s_server`, the nonce and the coordinate; token 2 is issued at reveal and additionally carries
the target, the set and its order.

**A five-column row is still written at trial creation** — sequence number, commitment `C`,
account identifier, pool version, status. The token does not replace it, for two reasons:

- **Replay.** A stateless server cannot tell it has seen a token before. Submit token 2 with
  image 1, learn "wrong"; resubmit with image 2, and so on. By the eighth attempt the target is
  known and a clean run can be filed. The commit row is the replay defence: a second answer for
  the same sequence number is refused.
- **It would silently revoke the Q1/B decision in the specification.** Abandoned trials are
  required to remain in the public record with a published abandonment rate (FR-014, FR-016,
  FR-021, FR-027, SC-012). With nothing written at creation, an abandonment leaves no trace at
  all — there is no gap to notice, because no sequence number was ever issued. D3's rule that
  logging at commit time makes aborts visible and countable would go with it.

So the session table this was meant to avoid **is** the audit log already committed to in D2.
There is no second table, and abandoned entries in it are a requirement rather than clutter.

What the token does buy is real, but it is a different prize than the one aimed at: `s_server`
never touches the database, so a backup contains no pending targets. That closes the caveat
recorded in D12.

**Abandonment needs no timer, but the token carries a maximum lifetime** of about 24 hours,
stored inside the authenticated payload and checked on redemption. The two are separate: a trial
is abandoned by not being completed, whatever the clock says; the lifetime governs how long it
*can still* be completed.

Three things this buys:

- Abandonment becomes final rather than provisional. Without it, a trial in progress and an
  abandoned trial stay indistinguishable forever, and the published abandonment rate never
  settles.
- **Key rotation becomes possible at all.** With no expiry, every token-encryption key would have
  to be retained indefinitely or old tokens become undecryptable. A 24-hour lifetime means the
  previous key is kept for one day and discarded. The lifetime should track the rotation
  interval, which is the real reason to pick a number.
- It bounds nothing else. The commit row is permanent regardless, so this is not a route to
  pruning the log.

The generous span costs no security: the holder has a blob they cannot decrypt, and holding
several open trials gains nothing because trials are independent. It buys tolerance for a viewer
who is interrupted mid-session.

Follows for the interface: a late answer must be explained as expired and offered a fresh trial,
never silently scored as a loss.

Noted for the record: statelessness was never actually reachable alongside "no time limit".
Any replay defence without a row must remember spent tokens, and without expiry it must remember
them forever — a larger version of the table being avoided.

## D17 — Planning parameters

Settled 2026-07-25 while preparing the implementation plan.

| Item | Decision |
|---|---|
| Abuse limits | Account creation limited per client IP; concurrent uncompleted trials capped per account |
| Commitment and derivation hash | SHA-256 — present in WebCrypto and in `sha2`, no extra browser dependency |
| Token encryption | XChaCha20-Poly1305; the 192-bit nonce removes any nonce-reuse concern without a counter |
| Derivation stream | `SHA-256(seed ‖ counter)`, consumed in 64-bit words, **rejection sampling** rather than modulo, decoys by partial Fisher-Yates |
| Log structure | Hash chain only — each entry carries `prev_hash`, the head is the last entry hash |
| Statistics block size | 25 completed trials |
| Leaderboard minimum | 100 completed trials, held as configuration and revisited against real distributions |
| Image pipeline | A Rust tool using the `image` crate, doubling as the operator's annotate-and-scale script |
| Deployment | Behind the existing nginx on the Hetzner host, alongside other sites |

Capping concurrent trials attacks the growth problem at its cause rather than rate-limiting
creation over time: a new trial requires an earlier one to be completed or expired, so the log
grows at most by the cap per account per token lifetime.

The derivation stream needs this much precision because D7 puts one implementation in Rust and
one in TypeScript. `pool[seed mod P]` alone is not a specification — decoy selection and display
order also have to be pinned, and rejection sampling is used because exactness is cheap and
verifiability is the entire point.

The image tool is not only a build step. It is the interface the operator uses when curating:
find an image, annotate its provenance and licence, scale and normalize it, and get back the
manifest entry.

**Two consequences of sitting behind a shared nginx, both of which silently break a decision if
missed:**

- The backend sees `127.0.0.1` as the client address unless nginx forwards the real one.
  IP-based limits on account creation would then be either inert or global, throttling every
  user together. nginx must set `X-Forwarded-For`, and the service must trust that header **only**
  from the proxy, otherwise any client can forge it.
- The `Host` header must be passed through, because D10 selects the locale bundle from it.
  Without it the language split fails.

## D18 — The premise: this is an experiment, and the null result is the expected one

From a second grilling session on 2026-07-25, this one about the concept rather than the
mechanism.

At 100,000 trials the standard error of the hit rate is 0.105%, so the 95% interval is ±0.21%.
The site will be able to detect a true rate from roughly 12.7% upward, and will almost certainly
display **12.50%**. The premise it exists to test will, in all likelihood, fail.

**This cannot be hidden.** The log is public and SC-004 requires any third party to be able to
recompute the aggregate. The choice was never "show or conceal" but "frame it yourself or let
critics frame it".

Position taken: vriltrainer is a **public experiment**, not a promise. A null result is the
result, not a failure. Every one of D1–D17 was built to avoid self-deception — server-side
targets, two-party randomness, statistics that refuse to filter on success, a public log. That
architecture is only coherent if the answer is allowed to be no.

It also works in the other direction: if the aggregate ever did land at 13.2%, the first question
anyone would ask is where the leak is, and D3 and D5 are the reason such a result could be
defended at all.

**Tone: the site does not take itself seriously.** The statistics stay accurate and defensible;
the delivery is memes. Telling users they are, statistically, normies is what makes the honest
outcome shareable — and it aligns the incentives, because the funny result and the truthful
result are the same one.

## D19 — Ranks are positional, and only exist once there is a population

> **Superseded by D23 (2026-07-25).** Positions were replaced by shares of the population, and
> the flat 200-account gate by a per-band rule. The reasoning below about *why* positional beat
> absolute thresholds still holds and is the reason D23 is not a return to thresholds — it is a
> refinement of the same insight. The rank names here are also out of date; D23 carries the
> current ladder.


| Rank | Position |
|---|---|
| Insektoider Archont | top 3 |
| Reptiloidenarchont | 4–10 |
| Grey Alien | 11–30 |
| Flugscheibenpilot | 31–80 |
| Psionic Asset | 81–200 |
| Normie | below that |
| Kartoffel | significantly below average |

Positional rather than absolute thresholds, which fixes a real defect. With statistics unlocking
at 10 trials, 2.75% of users reach z ≈ 2.6 in their first ten — one in thirty-six. Absolute
thresholds would have minted twenty-eight archons on a thousand-visitor launch day. Positional
supply is fixed: there are three top slots regardless of luck.

**Ranks activate only once 200 accounts are leaderboard-eligible** — eligible, not merely
registered. The tier table runs to rank 200, so it needs 200 ranked entries to mean anything;
counting registrations would leave the top three crowned among a dozen people. The gate doubles
as a recruitment incentive: "ranks unlock at 200 qualified, currently 47" gives early users a
reason to bring others.

What positional ranks cannot do is say *nobody here is special*. A top 3 exists even when the
aggregate is exactly 12.5% — those are simply the three luckiest. Displaying trial count and
z-value in the leaderboard mitigates this for readers of the page. For the **shared image** it
does not, so the trial count and the by-chance context belong **inside the graphic**, not beside
it. The same reasoning as the access link in D9: what users share must be honest on its own.

**The Kartoffel is the best scientific feature here.** Under the null, low outliers are exactly
as common as high ones. If the site keeps producing roughly as many potatoes as archons, that
ratio *is* the significance test — one anybody can read without statistics. Were psi real, the
upper tail would be heavier. Both counts are therefore displayed together.

## D20 — The leaderboard sorts by the Wilson lower bound, and shows it

Wilson is kept over the raw z-value, confirming D8. The two rank different things: z measures
evidence against chance, the Wilson bound estimates ability. A lucky run of 100 trials at 25%
gives z = 3.78 and would outrank a steadier 1000 trials at 15% (z = 2.39). Trial count matters,
so ability wins over surprise.

Because the sort key must be the number on display, the **Wilson lower bound is the headline
figure** — a percentage, so it reads naturally as "verified minimum rate: 17.7%" — with trial
count, hit rate and z-value as further columns. A leaderboard sorted by an invisible statistic
produces endless "why is that person above me".

Entry requires the 100 completed trials from D17.

## D21 — Friction, not barriers

**A minimum of three seconds between reveal and choice**, enforced server-side. The reveal
timestamp travels in token 2 from D16, which is issued at exactly that moment, so this needs no
additional state.

The rejection must happen **before evaluation** — the server answers "too fast" without looking
at the chosen image, or the rule becomes an oracle. A speed-rejected submission does not consume
the trial, which refines FR-037: at most one *evaluated* answer.

**Honest accounting of what this achieves:** not bot defence. The rule is per trial per account,
and a script defeats it with parallelism — a thousand accounts each waiting three seconds are all
done in about five minutes. What it actually buys is data quality: a trial answered in 200
milliseconds is click noise, not remote viewing, and it pollutes the aggregate. Good rule,
different reason than the one it was introduced for.

**Rank eligibility requires 100 trials across at least three distinct days.** This is the only
cheap measure that resists parallel farming, because parallelism does not compress the calendar.
A farm would have to keep a thousand accounts alive and playing across three days rather than
running them through over lunch — the cost moves from "script" to "infrastructure", which is
where most attackers stop.

For genuine users it describes what they already do; a trainer is used over weeks. It is also
better study design, since trials spread over sessions average out fatigue and warm-up instead of
compounding them. The cost is the enthusiast who plays 150 trials in one evening and is not
ranked the next morning — answered with a visible "2 more days until ranked" rather than silence.

No captchas, no proof-of-work, no verification. Those would destroy the frictionless entry
deliberately bought in D9.

Also considered and rejected: doing nothing. The ranks are scarce and desirable, so there is now
a motive. Farming a thousand accounts yields an expected best z of about 3.2 against an expected
honest maximum of 2.75 among 200 real participants — the top three would fall reliably. Note that
the **aggregate stays clean regardless**, because bot trials are genuine random draws; only the
leaderboard is corruptible.

## D22 — The candidate set is drawn across categories, and the target index is drawn last

Decoys were originally drawn uniformly from the whole pool, which leaves nothing preventing eight
near-identical images from appearing together. The scale of that was underestimated. With 500
images falling into motif groups of average size *s*, the expected number of confusable pairs per
trial is `28·(s−1)/499`:

| Motif groups | Average size | Expected pairs | Trials with a collision |
|---|---|---|---|
| 25 | 20 | 1.07 | **~66%** |
| 50 | 10 | 0.51 | ~39% |
| 100 | 5 | 0.22 | ~20% |

At realistic curation granularity, one trial in two or three contains a confusable pair. Not an
edge case.

**The obvious fix is a trap.** Partition the pool into exactly eight categories, draw the target
uniformly from the pool, then one decoy from each remaining category. If one category holds 200
images and another 20, the target is ten times more likely to come from the larger one — and
since exactly one image per category is shown, **the image from the larger category is ten times
more likely to be the target**. Anyone who noticed would play far above 12.5%, and the published
z-scores would report enormous psi that was pure bookkeeping. Eight fixed categories would also
make every trial look the same after a few hundred rounds.

**Chosen instead: more categories than images per trial, and the target index drawn last.**

```
K categories, 16 to 24, each holding roughly 20 images

1. choose 8 distinct categories        — partial Fisher-Yates over the K indices
2. choose one image per chosen category — uniform within that category's member list
3. choose the target index              — uniform over 0…7, in selection order
4. shuffle for display                  — Fisher-Yates over the eight
```

Step 3 is what matters. **The target is one of the eight shown, uniformly, independent of every
category size.** The bias is structurally impossible rather than carefully avoided, so categories
never need balancing and uneven growth can never hurt.

Consequences:

- **The manifest hash must cover the category assignment**, not only the identifier list.
  Otherwise a category could be reassigned without changing the hash, silently altering every
  future derivation while appearing unchanged.
- Category member lists are obtained by **filtering the sorted identifier list**, so there is only
  one normative ordering to agree on rather than two.
- The derivation gains two draw steps, so the D7 test vectors grow. All four steps come from the
  same counter stream with rejection sampling, so client-side recomputation is unaffected in kind.
- Curation gains a rule: every category needs enough images that repeats stay rare. At 24
  categories and roughly 20 images each, `P >= 500` still holds.

**Accepted deliberately:** the categories sit in the public manifest, so a viewer knows the eight
images will be eight different kinds of thing. This biases nothing, but it is a departure from
classical remote viewing protocol, where the viewer ideally knows nothing about the target set.
For a trainer it is judged a net gain — it gives the discrimination something to hold onto.

## Constraints

- **No Python.** Excludes the reference OpenTimestamps client, which is moot while D4 defers
  anchoring.
- Node is required regardless, as the Angular build toolchain. It is not currently installed
  on the development machine; Rust 1.95, Python 3.13, uv and sqlite3 are.

## D23 — Ranks are shares of the population, not positions

Supersedes D19. The ladder is eleven bands, symmetric around Normie:

| Rank | Band |
|---|---|
| Annunaki | best 0,1 % |
| Insektoider Loosh-Farmer | best 0,5 % |
| Reptiloidenarchont | best 2 % |
| Grey Alien | best 7 % |
| Psionisches Asset | best 20 % |
| Normie | the middle 60 % |
| Zirbeldrüse verkalkt | bottom 20 % |
| Erdstrahlen-Opfer | bottom 7 % |
| Orgonit-Enjoyer | bottom 2 % |
| Psi-Nullleiter | bottom 0,5 % |
| Kartoffel | bottom 0,1 % |

D19 was right that supply must be fixed rather than earned by hitting a threshold. It was wrong
to fix supply as a *seat count*. A seat does not mean the same thing at two population sizes:
third place out of ten is nothing, third place out of two hundred thousand is a title. A share
means the same thing at every size, which is the only form that survives the site growing.

**A band is awarded once `share x eligible >= 1`.** This replaces D19's flat 200-account gate with
a rule the bands derive themselves — best 20 % needs 5 eligible, best 7 % needs 15, best 2 % needs
50, best 0,5 % needs 200, best 0,1 % needs 1000. Ranks then appear progressively as the site grows
and every title means what it says on the day it first exists. There is deliberately no rounding
up: rounding would hand out the rarest title at any population, which is the opposite of what a
share is. The top rung is unreachable until a thousand people have taken this seriously, and that
is the correct joke.

**Ranks are recomputed server-side every ~15 minutes**, not per request and not frozen per block.
A materialised table keeps the board cheap and stable between recomputations, and the board states
when ranks were last updated — otherwise a rank that has not moved reads as a bug.

D19's closing argument carries over unchanged and is the reason the ladder is symmetric: under the
null, low outliers are exactly as common as high ones, so if the site keeps producing about as
many Kartoffeln as Annunaki, that ratio *is* the significance test, readable without statistics.
The symmetry is now visible in the ladder itself rather than only in the tail counts.

## D24 — Two processes, one machine, one database; the locale is a startup flag

Amends D10. `vriltrainer.de` and `vriltrainer.com` are served by **two instances of the same
binary**, each started with a hard `--locale de|en` switch that fixes which frontend it serves.
Both point at the same SQLite file.

This is simpler than selecting the bundle from the `Host` header, and it deletes a silent failure
mode: `deploy/nginx.conf` previously called `Host` load-bearing for language, which meant a
proxy misconfiguration served the wrong language rather than failing. A process that was started
as the German one cannot serve English by accident.

Sharing the database is what makes the two domains one product: one account table, one log, one
leaderboard, exactly as D10 confirmed. It also means the D9 access link **works on either
domain** — paste a `.de` link at `.com` and the token resolves against the same row. The D11
handoff therefore exists to switch *without pasting the secret*, not to make the account portable.

**This reopens R9 and must be got right.** R9 chose a single writer connection specifically so
that strictly increasing sequence numbers and a correct `prev_hash` chain would be trivially
correct "rather than a locking exercise". Two processes reinstate the locking exercise. Appending
to a hash chain is read-the-head-then-write, and two processes can read the same head and write
two entries claiming the same predecessor — a **forked audit log**, the one artefact this product
cannot get wrong, and one that passes every test on a quiet machine.

Required discipline, all four parts:

- every append inside `BEGIN IMMEDIATE`, taking the write lock *before* reading the head;
- `busy_timeout` set, so the loser waits instead of failing;
- `UNIQUE` on the sequence number and on `prev_hash`, so a lapse in the above **fails loudly**
  instead of forking silently;
- a chain walk at startup and in the nightly backup job.

Consequence for D12: two processes sharing a SQLite file means **one machine**. SQLite's locking
is unreliable over network filesystems, so the two domains cannot be split across hosts, and
there is no horizontal scaling path that keeps SQLite.

## D25 — A name is public only after it has been approved

`checkDisplayName` stops being the gate and becomes the **pre-filter**. It refuses the shapeless,
the reserved, addresses, hate terms and vulgarity — including through leet folding — so that what
reaches the review queue is only what survived. Everything that survives is then approved by a
human before it is shown to anyone else.

A name has state: `pending` -> `approved` | `rejected`, and the two audiences see different
things.

**The account holder always sees the name they chose** while it is under review. A **refused** name
is discarded rather than kept for them to look at again: holding a name you turned down is holding
personal data for no purpose, and the holder does not need the string echoed back — they need to
know it was refused and why, which the refusal code carries. An earlier draft of this decision said
both things in one paragraph, and the implementation followed the half SC-018 forbids.

**The public list shows the most recently approved name in clear text, and masks everything else.**
Masked, not replaced: a row reads as *a name exists here and has not been cleared yet* rather than
as an absence. The mask is a **fixed-length** run of dots, matching the masking idiom D9 already
uses for the access link. Fixed length is the point — a mask that preserved the real length and
first letter would still communicate the shape of a slur, which is precisely what pre-approval
exists to keep off the page. The public identifier is shown beside it either way (FR-029), so the
row is still attributable and still checkable against the log. On rename, the last **approved** name stays displayed until the new one
clears, so renaming is not punished with anonymity. A rejection is told to the user with a reason,
lets them choose again, and does **not** consume the rename rate limit. Rejected names are
discarded rather than retained — holding a name you refused is holding personal data for no
purpose.

**A decision is made about a name, not about an account.** Approve and reject each take the name
the reviewer read and apply only if it is still there; a holder who resubmits between the queue
being read and the button being pressed gets a no-op rather than a publication. Without that,
pre-approval is theatre — `approve(account_id)` publishes whatever the row holds at the moment of
the update, which is a string no human ever saw, and rejection clearing the rename cooldown makes
the window seconds rather than a day.

Erasure therefore carries its own state rather than being inferred from the name being absent,
because a refusal now clears it too. Conflating them would lock a refused holder out of ever
choosing again.

FR-026 is untouched: the log references the opaque account id and never the name, so none of this
reaches the record.

The honest cost is that the operator becomes a bottleneck. Every new player is masked until
somebody logs in, and that is worst exactly on the days the site is growing. The pre-filter keeps
the queue short; nothing keeps it off the calendar.

Review runs over a small **public** admin API — public rather than loopback because the reviewers
are not only the operator. Its blast radius is bounded by design instead of by authentication:
**the public admin API performs only reversible operations.** Approve and reject, nothing else.
Every destructive operation — deleting an account, touching the log, changing pool versions —
stays a CLI subcommand behind SSH. A leaked admin key therefore costs an embarrassing name on the
board for an hour, not the audit log, and the API needs no roles or scopes because there is only
one privilege level and it cannot do damage.

One key, its **hash** in the database and never the key itself, matching the D9 discipline for
player tokens. `server admin-key --rotate` writes a new hash and prints the key once. The hash
lives in the database rather than the environment file precisely so rotation needs no restart: a
rotation that costs downtime is a rotation that never happens.

## D26 — Thresholds are configuration, published, and expected to move

The statistics unlock count, the leaderboard eligibility floor and the D23 band edges are
**server configuration**, returned in the API responses that depend on them and stated on the
pages that display them. None of them is a constant in the client.

The reason is that they will move. At launch the site is unknown and the priority is users, so
the bars start low — the D17 eligibility floor of 100 trials across three distinct days stays as
it is — and they rise as activity justifies it. That is a deliberate trade: a low floor is
farmable (see D27) and an empty leaderboard is a worse launch than a gamed one.

What makes this fair rather than arbitrary is saying so first. The board states the thresholds in
force and that they will be adjusted while the site grows. Announce before raising, and nobody who
loses a rank has been ambushed.

## D27 — Multi-accounting is absorbed, not defended

One person can run many accounts. On a share-based leaderboard that is the obvious exploit: run
twenty, keep whichever got lucky, and that one is your Annunaki.

**No scoring rule fixes this, and the reason is the product's own thesis.** At true chance every
account's real value is identical — 12,5 %. There is no signal to rank, so any ordering is an
ordering of luck, and buying more tickets buys more luck. Concretely, with a 1000-trial budget:
ten accounts of 100 trials, keeping the best, expects about 17,6 hits and a Wilson lower bound
near 11,4 %, against 10,6 % for one account of 1000 trials. Splitting wins — the single account
regresses to the mean while the farmer keeps only the tail. Raising the confidence level to 99 %
flips it at ten accounts and loses again at a hundred, because the penalty at fixed `n` is fixed
while the max-of-K gain grows like sqrt(2 ln K).

D20's Wilson lower bound therefore stays, because it is the right thing to *show* — it is simply
not a defence. The only lever that scales with a farmer's effort is what an account costs to make
eligible, and D26 keeps that low on purpose for now.

So the position is: let people do what they want, and say so on the statistics page in the site's
own voice. Multi-accounting does not undermine the argument, it demonstrates it. Any real
countermeasure — device fingerprinting, email, payment — would cost the anonymity FR-001 exists to
protect, which is a much worse trade.

T082's simulated-population run should include an adversarial farmer, so this is measured rather
than argued; the figures above are a normal approximation to E[max of K], not a simulation.


## D28 — Logging: operational lines, aggregate counters, and nothing per person

The operator needs to know how many people are hitting the site. The product's promise is that it
does not know who they are. Both are satisfiable, but only by being deliberate about it, because
the default answer — an analytics script, or just keeping the access logs — quietly trades the
second for the first.

**Most of the question is already answered, publicly.** Every trial is recorded permanently in the
audit log with its timestamp, so accounts created per day, trials started, completed and abandoned,
hit rates and retention are all derivable from a file anybody can download. Nothing needs building
for that, and nothing about it is private that was not already public by design. What the log
cannot see is the visitor who never started a trial.

Three layers, and the boundary between them is what matters:

**1 — Request logs, structured, to stdout and therefore to journald.** One line per request with a
correlation id, method, the *matched route pattern* rather than the raw path, status, duration and
locale. The route pattern rather than the path because a path can carry an account identifier and a
log is the wrong place for one. Never the URL fragment — browsers do not send it, and the code
asserts that rather than assuming it (FR-006). Never a full referrer. These lines are for debugging
a 500 at three in the morning, not for counting anybody.

**2 — Daily aggregate counters in SQLite**: `daily_metric(day, locale, metric, count)`. Page views,
accounts created, trials started, completed and abandoned, names submitted and approved, proofs
opened, log downloads. Incremented in process. There is no per-visitor row to leak, subpoena or
regret, and the table is small enough to keep forever.

**3 — Unique visitors, counted without an identifier being retained.** The only honest way to count
people without tracking them: hash the client address with a salt that is generated at startup of
each day, held in memory and never written down, and keep the day's hashes in an in-memory set.
At midnight the size of the set is written to `daily_metric` and the set and the salt are discarded.
The count is real; the salt rotating daily makes it impossible to link a visitor across days; and
because nothing is persisted, there is nothing to hand over. An exact set rather than a
HyperLogLog sketch because at this scale exactness is free and a sketch is one more thing to
explain.

**Reading it is a CLI subcommand, not an endpoint.** `server metrics --since` over SSH. The public
admin API stays what D25 made it: name approval and nothing else. Publishing the traffic figures
alongside everything else this site publishes would be perfectly in character and can be added
later; it is not worth a new public surface on day one.

**nginx access logs are the exception and must be dealt with separately.** They record IP addresses
and the application does not. Set a short retention — days, not months — and say so in the privacy
notice, because that file is the only place a visitor's address is written down.

## Remaining actions

- ~~Create the seven rank artefacts.~~ **Done, and there are eleven** (D23). Original pixel work,
  owned outright, which is what closed the meme licensing question (research.md R10).

- Flip the repository to public (D6)
- Source and curate the image pool. The operator curates, using the annotate-and-scale tool
  from D17; the images themselves still have to be found (D5, D15)
- Write the derivation test vectors before the second implementation begins (D7, D15)

Rate limiting, reopened after D16, was settled in D17.
