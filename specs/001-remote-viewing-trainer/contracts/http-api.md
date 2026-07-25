# Contract — HTTP API

Consumed by the Angular client and, for the public endpoints, by anyone auditing the experiment.
Breaking changes to any of this require a major version bump (constitution, principle V).

**Authentication.** A bearer token in the `Authorization` header. The token reaches the browser
once, in a URL fragment, and is moved to local storage immediately; it **must never** appear in a
path or query string, because those are transmitted to the server and recorded (FR-006).

**Locale.** Selected from the `Host` header, not from a parameter and not from `Accept-Language`
(FR-028, FR-032). The proxy must forward `Host` unchanged.

## Account

### `POST /api/account`

Creates an account from a self-chosen name. Rate-limited per client address.

```jsonc
// request
{ "name": "otherfren" }
// 201
{ "public_id": "K7QF", "access_token": "…", "name": "otherfren" }
```

`access_token` is returned **once and never again**. There is no recovery (FR-005).
`429` when the per-address creation limit is exceeded.

### `DELETE /api/account/name`

Removes the display name. Authenticated by the access token, which is the only proof of ownership
that exists (FR-035). The account's trials remain in the log under its opaque identifier
(FR-036). `204` on success, idempotent.

## Trial

### `POST /api/trial`

Starts a trial. Writes the `COMMIT` entry before responding.

```jsonc
// 201
{
  "trial_id": "7f3a…",
  "coordinate": "4821-9037",
  "commitment": "sha256:…",          // H(s_server ‖ nonce ‖ coordinate)
  "pool_version": 3,
  "pool_manifest_hash": "sha256:…",
  "token": "…"                        // token 1, opaque to the client
}
```

`429` when the account already holds the maximum number of uncompleted trials (D17).

### `POST /api/trial/reveal`

Supplies the client's randomness and receives the candidate set. The target is fixed here — after
both contributions exist, before any choice (D1, D3).

```jsonc
// request
{ "token": "…", "s_client": "base64…" }   // 32 bytes from crypto.getRandomValues
// 200
{
  "images": ["img_9c2…", "img_4e1…", "…"],   // exactly 8, in derived display order
  "token": "…"                                // token 2, carries reveal time and expiry
}
```

The response body is the same shape whatever the target is; nothing here distinguishes it.

### `POST /api/trial/answer`

```jsonc
// request
{ "token": "…", "chosen": "img_4e1…" }
// 200
{
  "hit": false,
  "target": "img_9c2…",
  "s_server": "base64…",
  "nonce": "base64…",
  "seq": 1043
}
```

The reveal payload is what the client feeds to its own derivation to check the server (FR-019,
FR-020).

| Status | Meaning |
|---|---|
| `425` | Answered less than three seconds after reveal. **Rejected before the chosen image is examined**, so it leaks nothing (FR-039, SC-016). The trial stays open and may be answered again |
| `409` | This trial already has an evaluated answer (FR-037) |
| `410` | The trial's validity period has elapsed; it is permanently abandoned (FR-038) |

## Statistics and leaderboard

### `GET /api/stats/me` — authenticated

```jsonc
// 200 — before the threshold
{ "completed": 4, "unlocks_at": 10 }
// 200 — after
{
  "completed": 120, "hits": 21, "abandoned": 6,
  "hit_rate": 0.175, "deviation": 1.62,
  "by_chance_per_10k": 527,             // how many of 10,000 reach this by luck (R3)
  "wilson_lower": 0.117,
  "distinct_days": 4, "eligible": true,
  "rank": 41, "rank_title": "flugscheibenpilot"
}
```

`abandoned` is always present, so selective abandonment is visible rather than hidden (FR-021).
`rank` and `rank_title` are absent entirely while fewer than 200 accounts are eligible (FR-042).

### `GET /api/stats/aggregate` — public

```jsonc
{
  "trials": 148213, "hits": 18571, "hit_rate": 0.12529,
  "expected_rate": 0.125, "deviation": 0.34,
  "accounts": 1841, "abandoned": 9022,
  "tail_high": 7, "tail_low": 6          // the two tails, side by side (FR-043)
}
```

This is the headline figure, presented as such even when it is exactly chance (FR-045). The two
tail counts are the significance test a reader can perform by looking.

### `GET /api/leaderboard` — public

Sorted by `wilson_lower` descending among eligible accounts (D20). Every entry carries the sort
key as its primary figure plus the supporting numbers (FR-041).

```jsonc
{
  "ranks_active": true, "eligible_accounts": 214,
  "entries": [
    { "rank": 1, "title": "insektoider_archont", "name": "otherfren", "public_id": "K7QF",
      "wilson_lower": 0.181, "completed": 430, "hit_rate": 0.212, "deviation": 3.9 }
  ]
}
```

When `ranks_active` is false, `eligible_accounts` still reports progress toward 200.

## Public record

### `GET /api/log/head`

```jsonc
{ "seq": 148213, "entry_hash": "sha256:…", "as_of": "2026-07-25T18:03:11Z" }
```

### `GET /api/log?from=<seq>&limit=<n>`

The complete append-only record from a sequence number, `COMMIT` and `RESOLVE` entries in order
(FR-022). Format in [public-log.md](./public-log.md). No authentication — copies held by third
parties are the redundancy that partly substitutes for the deferred anchor (D4, D12).

### `GET /api/pool/{version}/manifest`

The published image list for a pool version. Format in [pool-manifest.md](./pool-manifest.md).
Required for anyone recomputing a trial.

## Language handoff

### `POST /api/handoff` — authenticated

`201 { "code": "…", "expires_in": 30 }` — single-use, short-lived.

### `POST /api/handoff/redeem`

`200 { "access_token": "…" }`, burning the code. This is how a session crosses the origin
boundary between the two domains without the long-lived token ever entering an address bar
(FR-027, D11). `410` if already used or expired.
