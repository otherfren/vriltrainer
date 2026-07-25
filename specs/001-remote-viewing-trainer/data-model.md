# Phase 1 — Data Model

Storage is one SQLite database. It is both the application store and the public audit log, so the
log tables are the ones whose shape is a contract rather than an implementation choice.

## The log is append-only, so a trial writes two entries

A trial changes state — created, then revealed, then answered — but an append-only chain cannot
have its entries edited afterwards. Mutating a row on answer would silently make the record
rewritable, which is the property D2 exists to prevent.

So each trial contributes **one commit entry** and **at most one resolve entry**:

```
seq 1041   COMMIT   trial 7f3a…   account a91c…   commitment C   pool v3
seq 1042   COMMIT   trial b204…   account 55e1…   commitment C'  pool v3
seq 1043   RESOLVE  trial 7f3a…   choice 5   target 2   miss   s_server  s_client  nonce
                                                       ↑ trial b204 never resolves — abandoned
```

**An abandoned trial is a commit entry with no matching resolve entry.** Nothing has to be marked
or swept; abandonment is the absence of a record, and it is countable by anyone holding the export
(FR-027, SC-012). This is also what makes selective abort by the operator visible: a missing
resolve is as conspicuous as a wrong one.

Every entry carries `prev_hash` and its own `entry_hash`, forming the chain whose latest value is
the published head (D17, hash chain rather than Merkle).

## Entities

### `account`

| Field | Notes |
|---|---|
| `id` | Internal opaque identifier. **This is what appears in the log** — never the name (FR-023) |
| `public_id` | Short identifier shown beside the name on the leaderboard (FR-029). Drawn independently; **not** derived from the access token (D9) |
| `token_hash` | Hash of the capability token. The token itself is never stored (D9) |
| `display_name` | Self-chosen, **nullable**. Removing it satisfies erasure while the log stays intact (FR-035, FR-036) |
| `created_at` | |

Validation: a name is not unique — collisions are expected and resolved for the reader by
`public_id`. Removal sets `display_name` to null and is irreversible from the interface.

### `log_entry`

| Field | Notes |
|---|---|
| `seq` | Monotonic, gapless. Gaps would themselves be evidence of tampering |
| `kind` | `COMMIT` or `RESOLVE` |
| `trial_id` | Links the pair |
| `account_id` | The opaque `account.id`, never the name |
| `created_at` | UTC. The three-day spread in FR-040 counts distinct UTC days from resolve entries (R4) |
| `prev_hash`, `entry_hash` | The chain |

`COMMIT` additionally carries the commitment `C`, the coordinate, and the `pool_version`.
`RESOLVE` additionally carries the chosen image, the target image, hit or miss, and the revealed
`s_server`, `s_client` and nonce — everything a third party needs to recompute the trial. Both
randomness contributions are published, so verification is open to anyone rather than only to the
participant whose browser produced `s_client`.

Note what is *absent* from `COMMIT`: `s_server`, the target, and the candidate set. Those live in
the encrypted token held by the client until the trial resolves (D16), which is why a database
backup contains no pending answers.

### `pool_version` and `pool_image`

| Entity | Fields |
|---|---|
| `pool_version` | `id`, `manifest_hash`, `image_count`, `created_at` |
| `pool_image` | `pool_version`, `index`, `image_id`, **`category`**, `source_url`, `licence`, `attribution` |

`image_id` is the hash of the **normalised** bytes, so identity follows content rather than
filename (D5). `manifest_hash` is a plain hash over the sorted `(image_id, category)` pairs — the
manifest is published whole, so a tree would buy nothing, but the category must be inside the hash
or it could be reassigned invisibly and change every future derivation (D22).

`category` is what lets a trial draw one image from each of eight distinct categories. It is a
curation judgement, not a computed property.

Provenance and licence are tracked per image but are **not** part of the manifest hash and are
never rendered next to a candidate image: any per-image annotation visible in the interface is a
sensory channel distinguishing the target from its decoys.

Extending the pool creates a new version. Trials reference the version they ran under, so earlier
trials stay verifiable forever.

### `handoff_code`

| Field | Notes |
|---|---|
| `code_hash` | Single-use, short-lived |
| `account_id` | |
| `expires_at` | Roughly 30 seconds |
| `used_at` | Redemption burns it |

Exists so the language switch can carry a session across an origin boundary without putting the
long-lived access token in the target URL (D11).

### `account_stats` — derived

Maintained incrementally on each resolve rather than computed per request: `completed`, `hits`,
`abandoned`, `distinct_utc_days`, `wilson_lower`, `deviation`, `eligible`.

The leaderboard sorts by `wilson_lower` among rows where `eligible` is true, and eligibility is
`completed >= 100 AND distinct_utc_days >= 3` (FR-040). Ranks are then assigned by position and
only rendered at all once 200 rows are eligible (FR-042).

## Trial state transitions

```
                    ┌──────────────────────────────── expired (validity elapsed)
                    │
created ──reveal──▶ revealed ──answer──▶ answered
   │                    │                    ▲
   │                    └── too fast (<3s) ──┘   rejected before evaluation,
   │                                             trial stays revealed (FR-039)
   └── never revealed, never answered ─────▶ abandoned (no resolve entry)
```

Rules that constrain these transitions:

- The target is fixed at **reveal**, from randomness both sides contributed, and before any
  choice exists (D1, D3).
- An answer arriving under three seconds is refused **without the chosen image being examined**,
  so the refusal carries no information about the target (FR-039, SC-016).
- At most one *evaluated* answer per trial; a speed-refused answer does not consume it (FR-037).
- Expiry is carried inside the token and checked on redemption (D16). After it, the trial can
  never resolve and is permanently abandoned.
- `abandoned` is not a stored state. It is the absence of a resolve entry, which is why it needs
  no timer and no cleanup job.
