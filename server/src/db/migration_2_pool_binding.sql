-- Migration 2 — a commit entry names the pool it was sealed against, not just a version number.
--
-- `pool_version` is a pointer. A version can be re-cut with different images under the same
-- number, and every trial recorded under it then verifies against whatever manifest the operator
-- serves today: the one place where "verifiable without trusting the operator" fell back on trust
-- (D34). The hash goes into the entry hash, so it is covered by the chain like everything else.

-- Nullable, and it has to be. Rows written before this migration hash to a preimage without the
-- field, and adding a value to them would change what they hash to — editing an append-only
-- record, which is the failure the whole table exists to prevent. They keep the shape they were
-- written in and stay verifiable; new rows carry the binding.
ALTER TABLE log_entry ADD COLUMN pool_manifest_hash TEXT;

-- The column being nullable must not become a way back out. This is the store-side half of the
-- rule `log::chain::verify` enforces on the downloaded file: once a commit carries the hash, every
-- later commit must. Written as a trigger rather than folded into the table CHECK because
-- rebuilding `log_entry` to widen the CHECK would re-validate the rows from before the binding and
-- refuse them — the migration would fail on precisely the history it must not touch.
CREATE TRIGGER log_entry_pool_binding_kept
BEFORE INSERT ON log_entry
WHEN NEW.kind = 'commit'
     AND NEW.pool_manifest_hash IS NULL
     AND EXISTS (
         SELECT 1 FROM log_entry
          WHERE kind = 'commit' AND pool_manifest_hash IS NOT NULL
     )
BEGIN
    SELECT RAISE(
        ABORT,
        'a commit entry must carry pool_manifest_hash once the log has begun carrying it'
    );
END;
