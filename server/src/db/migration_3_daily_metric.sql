-- Migration 3 — traffic counters (FR-052, D28).
--
-- Counts and nothing else. There is no row here for a visitor, a session, a path or an address:
-- one integer per day, per locale, per named event. That shape is the privacy property — a table
-- that cannot describe an individual cannot be subpoenaed for one, and it is the reason this site
-- can answer "how many people came yesterday" without keeping anything about who they were.
--
-- `unique_visitors` is the one figure that needs state to produce, and that state never reaches
-- this table: a daily salt and a set of hashes live in memory, the count is written at rollover,
-- and both are discarded (T113). Restarting the process therefore undercounts uniques for the
-- current day, which is the intended trade — the alternative is persisting the set.
CREATE TABLE daily_metric (
    -- `YYYY-MM-DD`, UTC. The same clock every other timestamp in this database uses.
    day    TEXT    NOT NULL,
    -- Which of the two processes counted it (D24). Both write here, and a figure that could not
    -- be attributed to a domain would be two audiences added together.
    locale TEXT    NOT NULL,
    metric TEXT    NOT NULL,
    count  INTEGER NOT NULL,
    PRIMARY KEY (day, locale, metric)
) WITHOUT ROWID;
