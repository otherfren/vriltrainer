# Contract — Public log format

The record third parties download, recompute and keep copies of. Its stability is the product's
central promise, so this format is versioned and breaking changes require a major bump.

Newline-delimited JSON, one entry per line, ordered by `seq` with no gaps.

## Entry shapes

```jsonc
// COMMIT — written before the outcome exists
{ "seq": 1041, "kind": "commit", "trial": "7f3a…", "account": "a91c…",
  "at": "2026-07-25T17:58:02Z", "coordinate": "4821-9037",
  "commitment": "sha256:…", "pool_version": 3,
  "prev": "sha256:…", "hash": "sha256:…" }

// RESOLVE — written when the trial is answered
{ "seq": 1043, "kind": "resolve", "trial": "7f3a…",
  "at": "2026-07-25T17:58:31Z", "chosen": "img_4e1…", "target": "img_9c2…",
  "hit": false, "s_server": "base64…", "s_client": "base64…", "nonce": "base64…",
  "prev": "sha256:…", "hash": "sha256:…" }
```

`account` is the opaque account identifier. **Self-chosen names never appear here** — that is what
lets a name be erased without invalidating a single entry (FR-023, FR-036, D13).

## What a verifier can check

1. **The chain.** `hash` over the entry's canonical serialisation with `prev` included; `prev` of
   entry *n* equals `hash` of entry *n−1*. A published head that matches the last line proves the
   file is the record the operator is standing behind.
2. **Each commitment.** `framed(s_server, nonce, coordinate)` — each field length-prefixed, see
   `shared/vectors/README.md` — must equal the `commitment` from
   the paired commit entry — proving the target was fixed before the choice, and that *this*
   coordinate belongs to *this* trial.
3. **Each derivation.** With `s_server` and `s_client` from the resolve entry and the pool
   manifest for `pool_version`, recompute target, decoys and display order (R1) and compare to
   `target`. Both contributions are published, so this is checkable by anyone, not only by the
   participant whose browser generated `s_client`.
4. **Abandonment.** A commit entry with no resolve entry is an abandoned trial. Counting them
   gives the abandonment rate, per account and overall (FR-027, SC-012).
5. **The aggregate.** Hits over resolves, which must reproduce the published headline figure
   (SC-004).

## One limit, stated plainly

The log proves it has not been *rewritten* only to the extent that observers hold earlier copies.
Anchoring was deferred (D4), and timestamping would not have closed the related gap anyway — it
establishes non-backdating, not uniqueness, so an operator could maintain two divergent logs and
stamp both. Detecting that needs readers comparing heads, which is why the export exists and why
the head is published separately.

Publishing `s_client` alongside `s_server` costs nothing once a trial has resolved: the target is
already in the same entry, so the randomness reveals no secret. It is what makes SC-002 true as
written — an independent party, not merely the participant, can recompute every trial in full.
