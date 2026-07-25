# Phase 0 — Research

Most of the design space was closed in two interviews recorded in
`docs/trial-protocol-decisions.md` (D1–D21). This document resolves what those left open, and
records the reasoning for choices that would otherwise look arbitrary in the code.

## R1 — Exact derivation stream

**Decision.** `seed = SHA-256(s_server ‖ s_client)`. From it, a keystream
`block(i) = SHA-256(seed ‖ LE64(i))` for `i = 0, 1, 2, …`, consumed as little-endian 64-bit
words. Uniform integers below `m` are drawn by **rejection sampling**: take the next word `w`,
reject it if `w >= floor(2^64 / m) * m`, otherwise return `w mod m`.

Four steps consume that stream in order (D22):

1. **Eight distinct categories** — partial Fisher-Yates over the `K` category indices.
2. **One image per chosen category** — uniform within that category's member list, which is the
   sorted manifest filtered to that category.
3. **The target index** — uniform over `0…7`, in *selection* order, before the display shuffle.
4. **Display order** — Fisher-Yates over the eight selected images.

**Rationale.** D7 puts one implementation in Rust and one in TypeScript, and they must agree
byte-for-byte or honest trials fail verification. Everything here is fixed: the hash, the counter
width and endianness, the word size, the rejection bound, the step order, and the shuffle
direction. `seed mod P` alone was not a specification — it says nothing about decoys or order,
which are equally part of what the client recomputes.

Step 3 exists to make bias impossible rather than merely avoided. Drawing the target from the
pool and then filling the other categories would make the target's likelihood proportional to its
category's size, so the image from the largest category would be the likeliest answer — a
strategy worth far more than 12.5% to anyone who noticed (D22).

**Alternatives considered.** HKDF-SHA256 as the stream: more standard, but adds a dependency on
both sides for no gain over counter-mode hashing. Modulo without rejection: the bias at
`P = 500` against a 64-bit word is around `10^-17` and utterly harmless — rejection was chosen
anyway because it is three lines, removes an argument, and the entire product is about not having
to trust a "harmless" shortcut.

## R2 — Wilson confidence level and the leaderboard statistic

**Decision.** Wilson score interval at **95%**, lower bound, as the sort key and the headline
figure per entry (D20).

**Rationale.** Wilson weighs trial count where the raw deviation does not, which is what the
leaderboard is supposed to reward. 95% is the convention and needs no defending in the interface.

**Alternatives considered.** Ranking by deviation from chance: rejected in D20 — it ranks
surprise rather than ability, so a lucky hundred-trial run outranks a steady thousand. A higher
confidence level such as 99.9% would separate the tiers more sharply, but the separation problem
is solved instead by the eligibility rule in FR-040, which is legible to users in a way a
confidence level is not.

## R3 — How the "by chance" context line is computed

**Decision.** For an account with `n` completed trials and `k` hits, report the one-sided
binomial tail `P(X >= k)` under `p = 0.125`, expressed as an expected count over the current
eligible population: *"about N of every 10,000 users reach this by chance."* Computed exactly
from the binomial for the sizes involved, not by normal approximation.

**Rationale.** FR-018 requires the deviation to arrive with the context that makes it
interpretable. A raw z-value means nothing to the audience; "13 in 10,000 get here by luck"
means something immediately. Exact binomial because `n` is often small enough that the normal
approximation misleads precisely where the claim is strongest.

## R4 — Definition of a "day" for the three-day spread

**Decision.** Distinct **UTC calendar days**, counted from the trial's recorded completion time.

**Rationale.** FR-040 needs an unambiguous rule that a third party can recompute from the public
log, which carries UTC timestamps. Local time would make eligibility depend on data the log does
not contain, and would let a user in the right timezone reach three days faster.

**Alternatives considered.** Rolling 24-hour windows: harder to explain and no more resistant to
farming. The cost of UTC days is a user near midnight local time who gains a "day" cheaply — a
factor-of-one advantage, irrelevant against the three-day requirement.

## R5 — What counts as markedly below chance (the Kartoffel)

**Decision.** Symmetric with the top of the ladder: the lower tail is defined by the same
statistical distance used for the upper one, applied downward, and displayed as a count next to
the upper tail's count (FR-043).

**Rationale.** The symmetry is the point. Under the null the two tails are equally populated, so
publishing both counts side by side is a significance test a reader performs by looking — no
statistics required. Defining the tails differently would destroy exactly that property, which is
the most legible honest signal the product has.

## R6 — Coordinate format

**Decision.** Eight decimal digits in two groups, `NNNN-NNNN`, drawn uniformly per trial and
carrying no information about the target.

**Rationale.** Remote viewing convention expects a coordinate, and the product would feel wrong
without one. It is a label bound into the commitment (D3) so the reveal proves *this* coordinate
pointed at *this* image; it is not an index into anything. Uniform random rather than sequential,
so it leaks no ordering.

## R7 — Trial token construction

**Decision.** XChaCha20-Poly1305, random 192-bit nonce per seal, key held only by the server.
The authenticated additional data binds the account identifier and the trial sequence number, so
a token cannot be replayed against a different account or trial. Token 1 carries `s_server`, the
commitment nonce and the coordinate. Token 2 additionally carries the target, the selected set,
the display order, the reveal timestamp and the expiry.

**Rationale.** The 192-bit nonce removes any nonce-management burden — random generation is safe
without a counter, which matters because the server is otherwise stateless between requests.
Binding account and sequence in the AAD is what makes swapping tokens between accounts fail
closed rather than silently.

**Alternatives considered.** AES-256-GCM: fine cryptographically but its 96-bit nonce makes random
generation a thing to reason about. JWT/JWE: a large specification surface for a token only this
server ever reads.

**Note.** Two token states are required rather than one because the candidate set does not exist
at trial start — it is derived from `s_client`, which arrives at reveal (D16).

## R8 — Deployment behind the existing nginx

**Decision.** The service binds to localhost; nginx terminates TLS for both domains and proxies
to it, forwarding `Host` unchanged and setting a client-address header that the service trusts
**only** from the proxy address.

**Rationale.** Both of these silently break a decision if omitted, which is why they are recorded
as configuration requirements rather than left to deployment day. Without a forwarded client
address every request appears to come from `127.0.0.1`, so the IP limit on account creation
becomes either inert or global — throttling all users together. Without an unchanged `Host` the
locale selection in D10 has nothing to select on. A service that trusted the header from any
source would let any client forge its own address, so the trust boundary is the proxy.

## R9 — SQLite under concurrent load

**Decision.** WAL mode, a single writer connection, a pool of readers. The log append and the
trial-status update happen in one transaction.

**Rationale.** The append-only log requires strictly increasing sequence numbers and a correct
`prev_hash` chain; serialising writes through one connection makes that trivially correct rather
than a locking exercise. Read traffic — leaderboard, statistics, log export — dominates and is
unaffected.

## R10 — Rank artwork and meme licensing

**Decision.** The operator creates the rank artwork and meme images himself. They are owned
outright, so there is nothing to license.

**Rationale.** D18 puts memes into the interface and D5 had restricted the trial pool to CC0
precisely because a `.de` domain makes casual reuse of found images a liability — the rank
artefacts are also the most *shared* images on the site (FR-044), which maximises both reach and
visibility to any rights holder. Producing them removes that exposure entirely, and the ranks are
the site's identity, which is worth owning rather than borrowing.

**Caveat worth stating once.** Original in the meme sense often means "I made this version" —
text laid over an existing template — in which case the underlying image still belongs to someone.
The decision recorded here is artwork original in the stricter sense. If a well-known template is
used as a base, this returns to being an open question.

**Alternatives considered.** Generated artwork: cheaper, with the usual provenance questions about
the generator. Public-domain source material, consistent with the trial pool. Accepting the risk
knowingly. All three are moot once the images are the operator's own.

## Open risks carried into planning

| Risk | Where it bites | Current answer |
|---|---|---|
| Meme licensing (R10) | P3, when ranks first render | Closed — artwork is the operator's own |
| Parallel account farming | Leaderboard credibility | Three-day spread (FR-040) raises cost; the aggregate is unaffected either way |
| Log growth from never-completed trials | Storage over years | Capped concurrent trials per account (D17); ~100 MB per million rows is not a real constraint |
| A genuine positive result | Reputation, and scrutiny | The architecture exists precisely so such a result could be defended; no further action |
