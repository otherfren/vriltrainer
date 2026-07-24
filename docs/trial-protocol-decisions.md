# Trial Protocol — Decisions

Running record of a `/grill-me` session held 2026-07-25, before any specification exists.
This file is **input** for the eventual `specs/001-*/spec.md` and the Phase 0 `research.md`;
it is not itself a Spec Kit artifact and carries no authority over them.

Status: session incomplete — 8 decisions settled, open questions listed at the bottom.

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
                  → client: coordinate, C = H(s_server ‖ nonce), pool_manifest_hash
2. Reveal click   client: s_client ← crypto.getRandomValues()
                  → server
                  seed   = H(s_server ‖ s_client)
                  target = pool[seed mod P]
                  decoys and display order derived from the same seed
                  → client: the N images
3. Pick           → server
4. Reveal         → client: s_server, nonce
                  client verifies H(s_server ‖ nonce) == C
                  client recomputes seed, target, decoys, order — must match
```

The commitment `C` **must** travel with the coordinate in step 1. A proof produced only at
step 4 verifies a claim the server was free to invent after seeing the pick, and is
worthless. The commitment binds the coordinate, so the reveal proves precisely the intended
statement: this coordinate pointed at this image.

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

## D5 — Public image pool, at least 500 images

The pool is public, versioned and hashed, as D3 requires: without it the client cannot
recompute a trial and the audit story collapses. `P >= 500` at launch.

Sizing rationale: with a public log, a user can look up image sets they have seen before. At
P = 500 and N = 8 (see D8) there are on the order of 10^17 possible sets, so exact repeats
effectively never occur. A small pool (P = 50) would let a lookup table accumulate within
days and would kill the leaderboard.

Pipeline requirements, all of them anti-leakage measures: fixed edge length, uniform
requantization, metadata stripped, opaque IDs derived from the normalized bytes rather than
filenames. Anything that distinguishes the target from its decoys — resolution, aspect
ratio, compression artifacts, the colour signature of a particular source — is a sensory
channel, and sensory leakage is the classic failure mode of forced-choice ESP experiments.

Manifest: sorted list of image IDs plus their Merkle root, which is the `pool_manifest_hash`
carried in each trial. Extending the pool creates a new version; older trials stay
verifiable against the version they were run under.

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

## Constraints

- **No Python.** Excludes the reference OpenTimestamps client, which is moot while D4 defers
  anchoring.
- Node is required regardless, as the Angular build toolchain. It is not currently installed
  on the development machine; Rust 1.95, Python 3.13, uv and sqlite3 are.

## Open questions

Not yet decided; the grilling session stopped here.

- Where the pool's several hundred images actually come from, and who curates them
- Identity: username-only, how it persists, what the leaderboard is worth without auth
- Licence choice, and flipping the repository to public
- i18n: which languages, and how the two domains relate to them
- Deployment target and operations
- MVP scope: which of the above is P1
