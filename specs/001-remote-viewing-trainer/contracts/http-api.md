# Contract — HTTP API

Consumed by the Angular client and, for the public endpoints, by anyone auditing the experiment.
Breaking changes to any of this require a major version bump (constitution, principle V).

**Authentication.** A bearer token in the `Authorization` header. The token reaches the browser
once, in a URL fragment, and is moved to local storage immediately; it **must never** appear in a
path or query string, because those are transmitted to the server and recorded (FR-006).

**Locale.** Fixed by the `--locale` flag each process was started with, never read off a request —
not from `Host`, not from a parameter, not from `Accept-Language` (FR-030, FR-032, D24). Two
processes serve the two domains; a foreign `Host` is a warning in the log, not a language switch.
Every response carries `Content-Language`.

## Account

### `POST /api/account`

Creates an account from a self-chosen name. Not rate-limited: the per-address creation cap of D17
was removed by D30.

```jsonc
// request
{ "name": "otherfren" }
// 201
{ "public_id": "7F3A9C", "access_token": "…", "name": "otherfren" }
```

`public_id` is six uppercase hex characters. `name` is the **stored** form — whitespace trimmed and
collapsed — and not necessarily what was typed, or the client displays a name the server does not
hold. The name starts `pending` and is masked on public surfaces until approved (D25, FR-047).

`access_token` is returned **once and never again**. There is no recovery (FR-005).
`400` when the name pre-filter refuses it, carrying the refusal code as `{ "error": "hate" }`.

### `GET /api/account` — authenticated

Who the bearer of this token is.

```jsonc
// 200
{ "public_id": "7F3A9C", "name": "otherfren", "name_state": "approved" }
// 200 — after erasure, or while no name has been approved and none was ever set
{ "public_id": "7F3A9C", "name": null, "name_state": "erased" }
```

`name` is the **holder's own** name in whatever review state it is in, exactly as `POST
/api/account` returns it: the holder is not a stranger and is never shown the mask (D25, FR-047).
`name_state` is `pending`, `approved`, `rejected` or `erased`, so the client can say *why* a name
is not on the board without guessing.

This exists because the access link is a capability and nothing else (D9). It carries a token, not
an identity — the token is a random 32 bytes and the name is not derivable from it, deliberately,
because a name in the fragment would travel into bookmark titles and shared links and would go
stale the moment the name changed or was erased. So a browser arriving through an access link
knows it has an account and knows nothing about whose it is, and without this endpoint the header
shows a placeholder for the rest of the session.

### `DELETE /api/account/name`

Removes the display name. Authenticated by the access token, which is the only proof of ownership
that exists (FR-035). The account's trials remain in the log under its opaque identifier
(FR-036). `204` on success, idempotent.

## Trial

### `POST /api/trial` — authenticated

Starts a trial. Writes the `COMMIT` entry before responding.

```jsonc
// 201
{
  "trial_id": "7f3a…",
  "coordinate": "4821-9037",
  "commitment": "sha256:…",          // framed(s_server, nonce, coordinate) — each field
                                      // length-prefixed, see public-log.md
  "pool_version": 3,
  "pool_manifest_hash": "sha256:…",
  "token": "…"                        // token 1, opaque to the client
}
```

There is no cap on how many trials an account may hold open at once — the D17 limit was removed by
D30. An unanswered trial still expires on the D16 clock and is still published as abandoned
(FR-021).

### `POST /api/trial/reveal` — authenticated

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

### `POST /api/trial/answer` — authenticated

```jsonc
// request
{ "token": "…", "chosen": "img_4e1…" }
// 200
{
  "hit": false,
  "target": "img_9c2…",
  "s_server": "base64…",
  "s_client": "base64…",
  "nonce": "base64…",
  "seq": 1043
}
```

The reveal payload is what the client feeds to its own derivation to check the server (FR-019,
FR-020). `s_client` is echoed although the browser produced it, so the verification panel checks
one payload rather than half a payload and half its own memory — and so does the resolve entry in
the log, for the same reason (D3, SC-002). `seq` names that entry.

| Status | Meaning |
|---|---|
| `400` | `chosen` is not one of the eight images shown, or the token does not parse. Nothing is written and the trial stays open — a choice the server never put on screen is refused rather than scored as a miss, because a resolve entry naming an image nobody saw is a line no reader of the log can make sense of |
| `425` | Answered less than three seconds after reveal. **Rejected before the chosen image is examined**, so it leaks nothing (FR-039, SC-016). The trial stays open and may be answered again |
| `409` | This trial already has an evaluated answer (FR-037) |
| `410` | The trial's validity period has elapsed; it is permanently abandoned (FR-038) |

## Statistics and leaderboard

### `GET /api/stats/me` — authenticated

```jsonc
// 200 — before the threshold
{ "completed": 4, "abandoned": 1, "unlocks_at": 10, "thresholds": { "…" } }
// 200 — after
{
  "completed": 120, "hits": 21, "abandoned": 6,
  "hit_rate": 0.175,
  "reported_trials": 120, "reported_hits": 21,  // the block the inferences stand over (FR-019)
  "deviation": 1.62,
  "by_chance_per_10k": 527,             // how many of 10,000 reach this by luck (R3)
  "wilson_lower": 0.117, "wilson_upper": 0.248,
  "distinct_days": 4, "eligible": true,
  "rank": "grey",
  "unlocks_at": 10,
  "thresholds": { "stats_unlock_at": 10, "eligibility_trials": 100, "eligibility_days": 3,
                  "bands": [ "…" ], "block_size": 10 }
}
```

`deviation`, `wilson_lower` and `wilson_upper` advance per block of ten completed trials and stand
over `reported_trials`/`reported_hits`, not over `completed`, which is why the two counts can
disagree and both be right (FR-019).

`abandoned` is always present, so selective abandonment is visible rather than hidden (FR-021).
`rank` is a band **slug**, not a position: a band is a fixed stretch of standard deviations, so
there is no seat number to report (D31, FR-042). The band follows from `deviation` alone and can be
rechecked against the published edges without the server's sort. The titles those slugs render as
are product copy and live in the client's message catalogue, one per domain. `rank` is absent for
the middle band — Normie, about a quarter of a chance population, and the honest answer for them.

`unlocks_at` and the band edges are configuration and are reported rather than assumed (D26,
FR-050).

### `GET /api/stats/aggregate` — public

```jsonc
{
  "trials": 148213, "hits": 18571, "hit_rate": 0.12529,
  "expected_rate": 0.125, "deviation": 0.34,
  "accounts": 1841, "abandoned": 9022,
  "tail_high": 7, "tail_low": 6,         // the two tails, side by side (FR-043)
  "qualified": 214,                      // accounts the distribution is over
  "tail_sigma": 1.9, "tail_min_trials": 10,
  "distribution": [                      // one column per rung, most negative first (D31)
    { "from": null, "to": -3.5, "accounts": 0, "tail": true, "rank": "kartoffel" },
    "…",
    { "from": -0.3, "to": 0.3, "accounts": 121, "tail": false, "rank": null },
    "…"
  ],
  "thresholds": { "…" }
}
```

This is the headline figure, presented as such even when it is exactly chance (FR-045). The two
tail counts are the significance test a reader can perform by looking, and `distribution` is the
same finding as a measurement: every qualified account binned into the sigma bands the ladder is
cut at, empty bands included, so a flat chart reads as the null rather than as a broken page.
`from` is `null` on the lowest band and `to` on the highest, which is how an open end is stated
rather than implied by some large number; `rank` is `null` for the middle one.
`tail_sigma` and `tail_min_trials` say what "markedly" means here and over what minimum record.

### `GET /api/leaderboard?offset=<n>&limit=<n>` — public

Paged. `offset` defaults to 0, `limit` to 20 and is clamped to 1..100; both are echoed back so a
client can page without keeping its own copy of the defaults.

Sorted by `wilson_lower` descending among eligible accounts, then `wilson_upper`, then `completed`,
then `public_id` (D20). Every entry carries both sort keys as its primary figures plus the
supporting numbers (FR-041): below chance `wilson_lower` is zero at every `n`, so the low tail is
ordered entirely on the ceiling, and a board sorted on something it does not show is the complaint
D20 settled.

```jsonc
{
  "eligible_accounts": 214,
  "bands_active": ["asset", "grey", "reptilian", "loosh", "annunaki"],
  "ranks_updated_at": "2026-07-25T18:00:00Z",
  "offset": 0,
  "limit": 20,
  "entries": [
    { "place": 1, "band": "reptilian", "name": "otherfren", "public_id": "7F3A9C",
      "wilson_lower": 0.181, "wilson_upper": 0.249, "completed": 430, "hit_rate": 0.212,
      "deviation": 3.9 }
  ],
  "thresholds": { "…" }
}
```

`band` is **absent** for an account in the middle band — Normie has no slug, the same absence
`rank` uses on `GET /api/stats/me`.

`name` is the most recently **approved** name, or a fixed-length mask if none has been approved yet
(FR-047, D25). The mask reveals neither the length nor the characters of what it hides; `public_id`
is shown beside it either way, so a masked row is still attributable and still checkable against
the log (FR-029).

`eligible_accounts` is reported whether or not anybody holds a title (FR-042). `bands_active` names
the ladder, nearest the middle first. Since D31 every rung exists at every population, so this is
the full list rather than a function of how many have played; it stays because readers use it to
see what the rungs above them are called. `ranks_updated_at` is when the ranks were last recomputed
— roughly every fifteen minutes — because a rank that has not moved otherwise reads as a bug.

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
Required for anyone recomputing a trial, and to be rehashed and held against the
`pool_manifest_hash` in the trial's commit entry before it is drawn against — the version number
is a pointer and can be re-cut (D34).

### `GET /pool/{image_id}.png`

The image bytes, served out of the binary (D29). Deliberately not under `/api`: this is what an
`<img src>` points at, and the client builds the address from the manifest's identifiers alone.
`image/png`, an ETag of the identifier, `Cache-Control: public, max-age=31536000, immutable` — the
identifier is the hash of the bytes, so a cached copy cannot go stale. Anything that is not a known
`<image_id>.png` is a `404`.

## Language handoff

### `POST /api/handoff` — authenticated

`201 { "code": "…", "expires_in": 30 }` — single-use, short-lived.

### `POST /api/handoff/redeem`

```jsonc
// request
{ "code": "…" }
// 200
{ "access_token": "…" }
```

Burning the code. This is how a session crosses the origin boundary between the two domains
without the long-lived token ever entering an address bar (FR-031, D11). `410` if already used,
expired, or never issued — the three are deliberately indistinguishable.

The token returned is a **new** one and the previous token stops working. That is forced rather
than chosen: only a hash of an access token is ever stored (D9), so the one the browser already
holds cannot be handed back, and storing it in a form that could be would put a usable credential
into every backup. One account holds one live token.

## Name review

The public admin API of D25, mounted under `/admin` rather than `/api`. Public because the
reviewers are not only the operator. Authenticated by a bearer **admin key**, which is a different
credential from a player's access token and is checked against a different table; its hash lives
in the database so `admin-key --rotate` takes effect without a restart.

**Reversible operations only.** Approve and reject a name, and nothing else — no deletion, no
access to the log, no pool changes. That, and not the authentication, is what bounds a leaked key.
Rate-limited per client address.

### `GET /admin/names?status=pending`

```jsonc
{ "status": "pending", "names": [ { "account_id": "…", "name": "otherfren" } ] }
```

Oldest submission first. `status` may only be `pending`; anything else is a `400`.

### `POST /admin/names/{account_id}/approve` · `POST /admin/names/{account_id}/reject`

```jsonc
// approve
{ "name": "otherfren" }
// reject
{ "name": "otherfren", "reason": "hate" }
// 200
{ "outcome": "applied" }
// 409
{ "error": "stale" }
```

`name` is **the name the reviewer read**, and it is not optional. The decision applies only if that
string is still the account's pending name; a holder who resubmits between the queue being read and
the button being pressed gets `409` and nothing is published (D25). A `409` means re-read the
queue, which is why it is not a quiet `200`.

`reason` is one of the pre-filter's codes — `too_short`, `too_long`, `shapeless`, `reserved`,
`hate`, `vulgar`, `address` — or `refused` for what a human turns down and the filter has no word
for. A closed list, because the code is rendered by the client's message catalogue and free text
here would be untranslated product copy stored in the database and shown to the holder verbatim.

## Health

### `GET /api/health`

```jsonc
// 200
{ "status": "ok", "seq": 148213, "locale": "de", "pool_version": 2 }
// 503
{ "status": "unavailable" }
```

`seq` is read from the chain head rather than returned as a constant, so a process that still
accepts connections while the database underneath it has gone reports `503` instead of looking
healthy. `503` rather than `500`, because it says "send traffic elsewhere", which is the one thing
a monitor and a proxy both know how to act on. Nothing here is secret: the head is published at
`GET /api/log/head` anyway.
