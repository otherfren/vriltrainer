-- Migration 1 — the whole schema, per specs/001-remote-viewing-trainer/data-model.md.
--
-- This file is not merely the application store. `log_entry` is the public audit log of D2, so
-- its shape is a contract with everyone who downloads the export, not an implementation choice.

-- Accounts. There is no registration, no email and no password: the access token is the account
-- (D9), and losing it is unrecoverable by design.
CREATE TABLE account (
    -- Opaque, internal. This is the only account reference that ever appears in the log.
    id              TEXT NOT NULL PRIMARY KEY,
    -- Shown beside the name on public surfaces (FR-029). Drawn independently — deriving it from
    -- the token would publish a function of the secret.
    public_id       TEXT NOT NULL UNIQUE,
    -- The access token is a password and is treated as one: only its hash is stored (D9), so a
    -- database backup carries no usable credentials.
    token_hash      TEXT NOT NULL UNIQUE,

    -- D25 name state. Two columns rather than one, because the two audiences see different
    -- things: `display_name` is what the holder sees in whatever state it is — they cannot pick a
    -- better name without seeing the one that was refused — while `public_name` is the last name
    -- a human approved and the only one a stranger ever sees. That split is what lets a rename
    -- keep the previous name on the board instead of punishing the rename with anonymity.
    -- Both go to NULL on erasure (FR-035); the log is untouched and stays verifiable (FR-036).
    display_name    TEXT,
    name_state      TEXT NOT NULL DEFAULT 'pending'
                    CHECK (name_state IN ('pending', 'approved', 'rejected', 'erased')),
    -- Machine-readable refusal code. The sentence shown to the user is product copy and lives in
    -- the client's catalogue.
    name_reason     TEXT,
    public_name     TEXT,
    -- When the holder last submitted a name, for the rename rate limit (FR-048). A rejection does
    -- not consume the limit, so this is set on submission and cleared by a rejection.
    name_changed_at TEXT,

    created_at      TEXT NOT NULL
);

CREATE INDEX account_name_queue ON account (name_state, created_at);

-- The audit log. Append-only: a trial contributes one COMMIT entry and at most one RESOLVE entry,
-- and an abandoned trial is a commit with no resolve. Nothing is marked or swept — abandonment is
-- the absence of a record, which is what makes it countable by anyone holding the export
-- (FR-027, SC-012) and makes a selective abort by the operator as conspicuous as a wrong answer.
CREATE TABLE log_entry (
    -- Monotonic and gapless. A gap is itself evidence of tampering.
    --
    -- This PRIMARY KEY and the UNIQUE on `prev_hash` below are the D24/R9 backstop, and they are
    -- the reason this table can be trusted at all. Two OS processes write to this file, one per
    -- domain. Appending to a hash chain is read-the-head-then-write, so two processes that read
    -- the same head write two entries claiming the same predecessor: a forked audit log. The
    -- append path prevents that with BEGIN IMMEDIATE; these two constraints ensure that if it
    -- ever stops preventing it, the second writer's INSERT fails loudly instead of forking
    -- silently. A fork passes every test on a quiet machine and appears only under load.
    seq          INTEGER NOT NULL PRIMARY KEY,
    kind         TEXT    NOT NULL CHECK (kind IN ('commit', 'resolve')),
    trial_id     TEXT    NOT NULL,
    -- The opaque account id, never the self-chosen name (FR-026, D13). A hash chain cannot have
    -- a name taken out of it afterwards, so no name ever goes in; erasure then costs a row in
    -- `account` and nothing here.
    --
    -- On a RESOLVE row this is a denormalised copy of the trial's COMMIT row, kept for queries.
    -- It is *not* part of the resolve entry's hash, so `verify_chain` says nothing about it and
    -- the matching commit row is the authority.
    account_id   TEXT    NOT NULL REFERENCES account (id),
    at           TEXT    NOT NULL,
    prev_hash    TEXT    NOT NULL UNIQUE,
    entry_hash   TEXT    NOT NULL UNIQUE,

    -- COMMIT only. Note what is absent: s_server, the target and the candidate set. Those live in
    -- the encrypted token held by the client (D16), which is why a backup contains no pending
    -- answers.
    coordinate   TEXT,
    commitment   TEXT,
    pool_version INTEGER,

    -- RESOLVE only. Both randomness contributions are published, so verification is open to any
    -- third party and not only to the participant whose browser produced s_client.
    chosen       TEXT,
    target       TEXT,
    hit          INTEGER CHECK (hit IN (0, 1)),
    s_server     TEXT,
    s_client     TEXT,
    nonce        TEXT,

    -- A row that carries the wrong half of the payload is a bug in the writer, and a bug in the
    -- writer of an append-only log is worth refusing rather than storing.
    CHECK (
        (kind = 'commit'
         AND coordinate IS NOT NULL AND commitment IS NOT NULL AND pool_version IS NOT NULL
         AND chosen IS NULL AND target IS NULL AND hit IS NULL
         AND s_server IS NULL AND s_client IS NULL AND nonce IS NULL)
        OR
        (kind = 'resolve'
         AND chosen IS NOT NULL AND target IS NOT NULL AND hit IS NOT NULL
         AND s_server IS NOT NULL AND s_client IS NOT NULL AND nonce IS NOT NULL
         AND coordinate IS NULL AND commitment IS NULL AND pool_version IS NULL)
    )
);

-- The replay defence reads this: a second evaluated answer for a trial is refused because its
-- resolve row already exists (FR-037, D16).
CREATE UNIQUE INDEX log_entry_trial_kind ON log_entry (trial_id, kind);
CREATE INDEX log_entry_account ON log_entry (account_id, kind);

-- Pool versions. Manifests are served for every version for as long as the service runs: a trial
-- recorded under v1 stays verifiable only while v1's manifest answers (D5). Image *bytes* are
-- replaceable — the derivation runs over ids, not bytes — which is what lets a takedown be
-- honoured without touching the log.
CREATE TABLE pool_version (
    id            INTEGER NOT NULL PRIMARY KEY,
    manifest_hash TEXT    NOT NULL,
    image_count   INTEGER NOT NULL,
    created_at    TEXT    NOT NULL
);

CREATE TABLE pool_image (
    pool_version INTEGER NOT NULL REFERENCES pool_version (id),
    -- Position in the sorted manifest. This ordering is what the derivation draws against, and
    -- category member lists are this list filtered — there is deliberately only one ordering to
    -- agree on between the two implementations (D22).
    idx          INTEGER NOT NULL,
    -- The hash of the normalised bytes, so identity follows content rather than filename (D5).
    image_id     TEXT    NOT NULL,
    -- Inside the manifest hash, or a category could be reassigned invisibly and silently alter
    -- every future derivation (D22).
    category     TEXT    NOT NULL,
    -- Tracked per image but never part of the manifest hash and never rendered beside a
    -- candidate: any per-image annotation visible in the interface is a sensory channel
    -- distinguishing the target from its decoys.
    source_url   TEXT,
    licence      TEXT,
    attribution  TEXT,
    PRIMARY KEY (pool_version, idx)
);

CREATE INDEX pool_image_category ON pool_image (pool_version, category, idx);

-- Single-use, ~30 second codes that carry a session across the origin boundary between the two
-- domains without the long-lived access token ever entering an address bar (D11, FR-031).
CREATE TABLE handoff_code (
    code_hash  TEXT NOT NULL PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES account (id),
    expires_at TEXT NOT NULL,
    used_at    TEXT
);

-- Derived, maintained incrementally on each resolve rather than computed per request.
CREATE TABLE account_stats (
    account_id        TEXT    NOT NULL PRIMARY KEY REFERENCES account (id),
    completed         INTEGER NOT NULL DEFAULT 0,
    hits              INTEGER NOT NULL DEFAULT 0,
    -- Always maintained and always reported, so selective abandonment is visible rather than
    -- hidden (FR-016, FR-021).
    abandoned         INTEGER NOT NULL DEFAULT 0,
    distinct_utc_days INTEGER NOT NULL DEFAULT 0,
    -- Counting distinct days needs no side table: a resolve always happens now, so the day is
    -- either the one already counted or a new one (FR-040, R4).
    last_utc_day      TEXT,
    wilson_lower      REAL    NOT NULL DEFAULT 0,
    deviation         REAL    NOT NULL DEFAULT 0,
    eligible          INTEGER NOT NULL DEFAULT 0 CHECK (eligible IN (0, 1)),
    -- Materialised by the ~15-minute rank pass (D23), so the board is a read of one table. The
    -- board states when this last ran, or a rank that has not moved reads as a bug.
    rank_slug         TEXT,
    ranked_at         TEXT,
    updated_at        TEXT    NOT NULL
);

CREATE INDEX account_stats_board ON account_stats (eligible, wilson_lower DESC);

-- Keys for the public admin API of D25. One privilege level, because the API performs only
-- reversible operations — approve and reject a name, nothing else. Everything destructive stays a
-- CLI subcommand behind SSH, so a leaked key costs an embarrassing name on the board for an hour
-- and not the audit log.
--
-- The hash lives here and not in an environment file precisely so rotation needs no restart: a
-- rotation that costs downtime is a rotation that never happens. The key itself is never stored,
-- the same discipline D9 applies to player tokens.
CREATE TABLE admin_key (
    id           TEXT NOT NULL PRIMARY KEY,
    label        TEXT NOT NULL,
    hash         TEXT NOT NULL UNIQUE,
    created_at   TEXT NOT NULL,
    revoked_at   TEXT,
    last_used_at TEXT
);
