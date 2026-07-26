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
| `display_name` | Self-chosen, **nullable**. What the holder sees, in whatever state it is (D25) |
| `public_name` | The last name a human approved, and the only name a stranger ever sees (D25, FR-047) |
| `name_state` | `pending`, `approved`, `rejected` or `erased`. Erasure is marked here, not by nullness |
| `name_reason` | Machine-readable refusal code; the sentence shown to the user is client copy |
| `name_changed_at` | Last submission, for the rename rate limit (FR-048). A rejection clears it |
| `created_at` | |

Validation: a name is not unique — collisions are expected and resolved for the reader by
`public_id`. Erasure sets both `display_name` and `public_name` to null and `name_state` to
`erased`, which satisfies erasure while the log stays intact (FR-035, FR-036). It is irreversible
from the interface, and the permanence is carried by the state rather than by nullness, because a
rejection clears the name too.

### `log_entry`

| Field | Notes |
|---|---|
| `seq` | Monotonic, gapless. Gaps would themselves be evidence of tampering |
| `kind` | `commit` or `resolve` — the literals the CHECK constraint and the export carry; the uppercase spelling elsewhere in this document is typographic |
| `trial_id` | Links the pair |
| `account_id` | The opaque `account.id`, never the name |
| `at` | UTC. The three-day spread in FR-040 counts distinct UTC days from resolve entries (R4) |
| `prev_hash`, `entry_hash` | The chain |

`COMMIT` additionally carries the commitment `C`, the coordinate, the `pool_version` and the
`pool_manifest_hash` that version stood for when the trial was sealed. The number alone is a
pointer and can be re-cut; the hash is what a reader checks the served manifest against (D34). It
is `NULL` only on rows written before migration 2, which were left as they were rather than
rewritten — see `contracts/public-log.md` for how a verifier handles that without a switch-over
sequence number.
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
| `pool_image` | `pool_version`, `idx`, `image_id`, **`category`**, `source_url`, `licence`, `attribution` |

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
`abandoned`, `distinct_utc_days`, `last_utc_day`, `updated_at`, and at a block boundary
`wilson_lower`, `wilson_upper`, `deviation` and `rank_slug`. `eligible` and `ranked_at` are not
written on resolve: they are materialised by the ~15-minute rank pass (D23, D31), which is why the
board states when that pass last ran.

The leaderboard sorts by `wilson_lower` among rows where `eligible` is true, tie-broken by
`wilson_upper`, then `completed`, then `public_id`, and eligibility is
`completed >= 100 AND distinct_utc_days >= 3` (FR-040). A rank is independent of that order: it is
a fixed band of standard deviations read off the account's own `deviation` as soon as it has
`stats_unlock_at` completed trials, so every rung exists at every population and no stranger can
move it (FR-042, D31).

### `admin_key`

| Field | Notes |
|---|---|
| `id` | |
| `label` | Who or what the key was cut for |
| `hash` | The key itself is never stored, the same discipline as D9 |
| `created_at`, `revoked_at` | Retired rather than deleted, so the old row still answers "was it used?" |
| `last_used_at` | Stamped on each authenticated call |

One privilege level, because the public admin API of D25 performs only reversible operations —
approve and reject a name. The hash lives here rather than in an environment file so rotation needs
no restart.

Migrations themselves are tracked in `schema_version`, written by the migration runner.

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
