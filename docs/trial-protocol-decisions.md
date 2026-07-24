# Trial Protocol — Decisions

Running record of a `/grill-me` session held 2026-07-25, before any specification exists.
This file is **input** for the eventual `specs/001-*/spec.md` and the Phase 0 `research.md`;
it is not itself a Spec Kit artifact and carries no authority over them.

Status: session incomplete — 4 decisions settled, open questions listed at the bottom.

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

## D4 — No blockchain for randomness; OpenTimestamps for anchoring

Public randomness beacons (drand, NIST, blockhash, RANDAO) are unusable here: their values
are published, so any user could compute the target. The only variant that avoids this uses
a beacon round that does not yet exist at viewing time, which converts the experiment into
precognition and contradicts D1.

For log integrity, a Merkle root of the append-only log stamped periodically via
OpenTimestamps reaches level C at no cost — aggregated, Bitcoin-backed, no wallet, no fees,
no per-trial chain interaction.

## Open questions

Not yet decided; the grilling session stopped here.

- Image pool: source and licensing for public hosting on two domains, pool size `P`,
  normalization to prevent sensory leakage, manifest format and versioning
- `N` images per trial, and therefore the chance rate the z-score is measured against
- Statistics: per-user or cumulative, handling of multiple comparisons across many users,
  optional stopping, what is claimed in the UI
- Identity: username-only, how it persists, what the leaderboard is worth without auth
- Backend language and storage
- i18n: which languages, and how the two domains relate to them
- Deployment target and operations
- MVP scope: which of the above is P1
