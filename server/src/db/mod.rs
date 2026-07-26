//! One SQLite file, holding both the application store and the public audit log.
//!
//! Two OS processes write to it — one per domain, since D24 made the locale a startup flag — and
//! that is the fact every decision in this module answers to. See [`Db::append_with`] for the
//! append discipline, which is the part that matters.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, Row, Transaction, TransactionBehavior, params};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::log::chain::{Body, ChainError, Entry, GENESIS, entry_hash};

/// How long a writer waits for the other process's lock before giving up.
///
/// D24 requires this to be set at all: with SQLite's default of zero the process that loses the
/// race fails the request outright rather than waiting the millisecond it takes the other
/// transaction to commit. Generous, because the alternative to waiting is a lost trial.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Read connections kept alive between requests. Read traffic — leaderboard, statistics, log
/// export — dominates and never touches the write lock, so this is the cheap half.
const POOLED_READERS: usize = 4;

/// Migrations, applied in order. `include_str!` rather than a string literal so the schema stays
/// readable as SQL and diffs as SQL.
const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("schema.sql")),
    (2, include_str!("migration_2_pool_binding.sql")),
];

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The audit log does not hold together. Never a routine condition: this is the failure the
    /// whole product is built to make impossible.
    #[error("the audit log is broken: {0}")]
    Chain(#[from] ChainError),
    #[error("migration {version} failed: {message}")]
    Migration { version: u32, message: String },
    #[error("PRAGMA {name} is {got}, expected {want}")]
    Pragma {
        name: &'static str,
        got: String,
        want: &'static str,
    },
    #[error("log entry {seq} has kind {kind}, which no reader knows how to read")]
    UnknownEntryKind { seq: u64, kind: String },
    /// A resolve entry whose trial was never committed. The pair is the whole record of a trial,
    /// so half of it is not a state worth writing.
    #[error("trial {trial} has no commit entry to resolve")]
    OrphanResolve { trial: String },
    /// An append the caller's own check refused after seeing the log from inside the write lock.
    ///
    /// [`Db::append_with`] inserts before it runs `also`, so this is the only way back out. The
    /// concurrent-trial cap of D17 needs it: counting a caller's open trials is read-then-write,
    /// and a count taken before the transaction lets two parallel requests both pass a full cap —
    /// which is exactly the case a cap on a permanent log exists for.
    #[error("the append was refused: {0}")]
    Vetoed(&'static str),
}

/// Now, as the log and the API spell it: RFC 3339, UTC, whole seconds.
///
/// Whole seconds because the timestamp is inside the entry hash, and two implementations agreeing
/// on a hash must first agree on the string — sub-second precision is one more thing to get wrong
/// for no gain.
pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .expect("zero nanoseconds is in range")
        .format(&Rfc3339)
        .expect("an RFC 3339 timestamp formats")
}

/// The `YYYY-MM-DD` prefix of a log timestamp. The three-day spread of FR-040 counts these.
pub fn utc_day(rfc3339: &str) -> &str {
    &rfc3339[..rfc3339.len().min(10)]
}

/// A poisoned lock here means a previous caller panicked while holding it. The transaction guard
/// rolls back as it unwinds, so what remains is a connection with nothing open — refusing to serve
/// from then on would turn one panic into an outage.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct Db {
    path: PathBuf,
    in_memory: bool,
    /// The single writer of *this* process. It does not make appends safe on its own — the other
    /// domain's process holds one too — but it keeps this process from queueing on SQLite's lock
    /// against itself.
    writer: Mutex<Connection>,
    readers: Mutex<Vec<Connection>>,
}

impl Db {
    /// Opens the file, applies pending migrations, and hands back a handle.
    ///
    /// Does **not** verify the chain: that is [`Db::verify_chain`], called explicitly at startup
    /// and by the backup job, so that a walk of the whole log is never an invisible cost.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let writer = Self::connect_writer(path)?;
        let db = Db {
            path: path.to_path_buf(),
            in_memory: false,
            writer: Mutex::new(writer),
            readers: Mutex::new(Vec::new()),
        };
        db.migrate()?;
        Ok(db)
    }

    /// An in-memory database, for tests.
    ///
    /// It exists only inside one connection, so reads are served by the writer and a reader held
    /// across an append would deadlock. It also cannot show the two-process behaviour of D24 —
    /// which is why the concurrency test uses a file.
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Db {
            path: PathBuf::from(":memory:"),
            in_memory: true,
            writer: Mutex::new(conn),
            readers: Mutex::new(Vec::new()),
        };
        db.migrate()?;
        Ok(db)
    }

    fn connect_writer(path: &Path) -> Result<Connection, DbError> {
        let conn = Connection::open(path)?;

        // Before the first statement that can block, not after. Converting the journal to WAL
        // takes a brief exclusive lock, and with D24's two processes started from one systemd
        // target they reach it together on a fresh file. Set later, that conversion runs under
        // SQLite's default zero busy handler and one of the two refuses to start.
        conn.busy_timeout(BUSY_TIMEOUT)?;

        // WAL, so readers never block the writer and the writer never blocks readers. With two
        // writing processes this is not a performance choice: under the rollback journal a reader
        // holding a shared lock stalls the other domain's append.
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        if mode != "wal" {
            // Reached if the file sits on a network filesystem, where SQLite's locking is
            // unreliable anyway — the situation D24 forbids. Better to refuse to start.
            return Err(DbError::Pragma {
                name: "journal_mode",
                got: mode,
                want: "wal",
            });
        }
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // The log is the product. Losing the last few entries to a power cut is not a tolerable
        // trade for write throughput a site of this size will never need.
        conn.pragma_update(None, "synchronous", "FULL")?;
        Ok(conn)
    }

    fn connect_reader(path: &Path) -> Result<Connection, DbError> {
        // Read-only at the SQLite level, not by convention: it makes a write attempted outside
        // the append discipline an immediate error rather than a silent third writer.
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        Ok(conn)
    }

    /// Even one migration file needs a version table.
    ///
    /// Retrofitting one onto a live audit log means guessing which shape the file on disk already
    /// has, and guessing wrong there is a restore from backup — of the one file whose loss makes
    /// every past trial unverifiable (D12).
    fn migrate(&self) -> Result<(), DbError> {
        let mut w = lock(&self.writer);
        w.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                 version    INTEGER NOT NULL PRIMARY KEY,
                 applied_at TEXT    NOT NULL
             )",
        )?;

        // The version is read INSIDE the write lock, not before it. Read in autocommit, this is
        // structurally the same read-then-write race the append path exists to avoid: both
        // processes see version 0, both enter the loop, and the loser dies on `table account
        // already exists`. D24 starts two processes from one target, so they meet here on every
        // fresh deployment — measured at roughly one failure in seven cold starts before this.
        let tx = w.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: u32 = tx.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )?;

        for (version, sql) in MIGRATIONS {
            if *version <= current {
                continue;
            }
            tx.execute_batch(sql).map_err(|e| DbError::Migration {
                version: *version,
                message: e.to_string(),
            })?;
            tx.execute(
                "INSERT INTO schema_version (version, applied_at) VALUES (?1, ?2)",
                params![version, now_rfc3339()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Borrows a read-only connection, returning it to the pool on drop.
    pub fn reader(&self) -> Result<Reader<'_>, DbError> {
        // An in-memory database exists only inside its own connection, so there is no second one
        // to open and the writer serves reads. That holds the writer lock for the life of the
        // reader — fine for a test, which is the only place this variant is reachable, and the
        // reason `open_in_memory` says so.
        if self.in_memory {
            return Ok(Reader {
                held: Held::Writer(lock(&self.writer)),
            });
        }
        let conn = match lock(&self.readers).pop() {
            Some(c) => c,
            None => Self::connect_reader(&self.path)?,
        };
        Ok(Reader {
            held: Held::Pooled {
                conn: Some(conn),
                db: self,
            },
        })
    }

    /// Appends one entry to the chain and runs `also` inside the same transaction, so the record
    /// and the state it describes commit together or not at all.
    ///
    /// # The append discipline (R9, rewritten for D24)
    ///
    /// The transaction is opened `IMMEDIATE`, which takes SQLite's write lock **before** the head
    /// is read. This looks like over-engineering. It is not, and the next person to read it should
    /// know why before deciding otherwise.
    ///
    /// Two OS processes write to this file, one per domain. Appending to a hash chain is
    /// read-the-head-then-write. Under a deferred transaction — SQLite's default, and what a plain
    /// `BEGIN` gives — both processes read the same head, both compute `prev_hash` from it, and
    /// both write: a forked audit log, the one artefact this product cannot get wrong. It passes
    /// every test on a quiet machine and appears only under concurrency, which is to say on the
    /// day the site is busiest.
    ///
    /// `busy_timeout` makes the loser of the lock wait instead of failing. The `UNIQUE`
    /// constraints on `seq` and `prev_hash` in `schema.sql` are the backstop: if this discipline
    /// is ever lost, the second writer's `INSERT` fails loudly instead of forking silently.
    pub fn append_with<T>(
        &self,
        at: &str,
        body: Body,
        also: impl FnOnce(&Transaction<'_>, &Entry) -> Result<T, DbError>,
    ) -> Result<(Entry, T), DbError> {
        let mut w = lock(&self.writer);
        let tx = w.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let (last_seq, prev) = chain_head(&tx)?;
        let seq = last_seq + 1;
        let hash = entry_hash(&prev, seq, at, &body);
        let entry = Entry {
            seq,
            at: at.to_string(),
            body,
            prev,
            hash,
        };

        insert_entry(&tx, &entry)?;
        let extra = also(&tx, &entry)?;
        tx.commit()?;
        Ok((entry, extra))
    }

    /// [`Db::append_with`] with nothing else to commit alongside.
    pub fn append(&self, at: &str, body: Body) -> Result<Entry, DbError> {
        self.append_with(at, body, |_, _| Ok(()))
            .map(|(entry, ())| entry)
    }

    /// A write transaction with no log entry to ride along with.
    ///
    /// Accounts, names, handoff codes and admin keys are exactly the state the log deliberately
    /// does not carry (FR-026), so they have nothing to pass to [`Db::append_with`]. `IMMEDIATE`
    /// for the same reason that path uses it: the other domain's process may be appending, and a
    /// deferred transaction that upgrades to a write half way through fails with
    /// `SQLITE_BUSY_SNAPSHOT`, which `busy_timeout` deliberately does not retry.
    ///
    /// Reads inside `f` must go through the transaction. An in-memory database serves
    /// [`Db::reader`] from this very connection, so taking one here deadlocks.
    pub fn write<T>(
        &self,
        f: impl FnOnce(&Transaction<'_>) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        let mut w = lock(&self.writer);
        let tx = w.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// The published head: the last entry's sequence number and hash, or `(0, GENESIS)` on an
    /// empty chain.
    pub fn head(&self) -> Result<(u64, String), DbError> {
        let r = self.reader()?;
        chain_head(&r)
    }

    /// Entries from `from` inclusive, in sequence order. The export (FR-025) reads through this.
    pub fn entries_from(&self, from: u64, limit: u64) -> Result<Vec<Entry>, DbError> {
        let r = self.reader()?;
        let mut stmt = r.prepare(&format!(
            "SELECT {ENTRY_COLUMNS} FROM log_entry WHERE seq >= ?1 ORDER BY seq LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![from, limit], entry_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row??);
        }
        Ok(out)
    }

    /// Walks the chain from genesis and confirms every `prev_hash` links, every hash recomputes,
    /// and no sequence number is missing. Returns the number of entries checked.
    ///
    /// Called at startup and available to the backup job, as D24 requires. The `UNIQUE`
    /// constraints catch a fork at the moment of writing; this catches everything that reached the
    /// file some other way — a hand-edited row, a partial restore, a merge of two backups — none
    /// of which the constraints would notice.
    ///
    /// Streamed rather than loaded and handed to [`crate::log::chain::verify`]: the log grows
    /// without bound and a startup check that needs it all in memory stops being run.
    pub fn verify_chain(&self) -> Result<u64, DbError> {
        let r = self.reader()?;
        let mut stmt = r.prepare(&format!(
            "SELECT {ENTRY_COLUMNS} FROM log_entry ORDER BY seq"
        ))?;
        let mut rows = stmt.query([])?;

        let mut prev = GENESIS.to_string();
        let mut expected: u64 = 1;
        while let Some(row) = rows.next()? {
            let e = entry_from_row(row)??;
            if e.seq != expected {
                return Err(ChainError::SequenceBroken {
                    at: e.seq,
                    expected,
                }
                .into());
            }
            if e.prev != prev {
                return Err(ChainError::PrevMismatch { seq: e.seq }.into());
            }
            if entry_hash(&e.prev, e.seq, &e.at, &e.body) != e.hash {
                return Err(ChainError::HashMismatch { seq: e.seq }.into());
            }
            prev = e.hash;
            expected += 1;
        }
        Ok(expected - 1)
    }
}

/// A borrowed read-only connection. Returned to the pool when dropped.
pub struct Reader<'db> {
    held: Held<'db>,
}

enum Held<'db> {
    Pooled {
        conn: Option<Connection>,
        db: &'db Db,
    },
    /// The writer, for an in-memory database. See [`Db::reader`].
    Writer(std::sync::MutexGuard<'db, Connection>),
}

impl std::ops::Deref for Reader<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        match &self.held {
            Held::Pooled { conn, .. } => conn.as_ref().expect("held until drop"),
            Held::Writer(guard) => guard,
        }
    }
}

impl Drop for Reader<'_> {
    fn drop(&mut self) {
        if let Held::Pooled { conn, db } = &mut self.held
            && let Some(conn) = conn.take()
        {
            let mut pool = lock(&db.readers);
            if pool.len() < POOLED_READERS {
                pool.push(conn);
            }
        }
    }
}

const ENTRY_COLUMNS: &str = "seq, kind, trial_id, account_id, at, prev_hash, entry_hash, \
                             coordinate, commitment, pool_version, \
                             chosen, target, hit, s_server, s_client, nonce, \
                             pool_manifest_hash";

/// The last entry's sequence number and hash, or `(0, GENESIS)`.
///
/// Called **inside** the write transaction on the append path. Calling it outside is what forks
/// the chain — see [`Db::append_with`].
fn chain_head(conn: &Connection) -> Result<(u64, String), DbError> {
    let found: Option<(u64, String)> = conn
        .query_row(
            "SELECT seq, entry_hash FROM log_entry ORDER BY seq DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(found.unwrap_or((0, GENESIS.to_string())))
}

fn insert_entry(tx: &Transaction<'_>, entry: &Entry) -> Result<(), DbError> {
    match &entry.body {
        Body::Commit {
            trial,
            account,
            coordinate,
            commitment,
            pool_version,
            pool_manifest_hash,
        } => {
            tx.execute(
                "INSERT INTO log_entry
                   (seq, kind, trial_id, account_id, at, prev_hash, entry_hash,
                    coordinate, commitment, pool_version, pool_manifest_hash)
                 VALUES (?1, 'commit', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    entry.seq,
                    trial,
                    account,
                    entry.at,
                    entry.prev,
                    entry.hash,
                    coordinate,
                    commitment,
                    pool_version,
                    pool_manifest_hash
                ],
            )?;
        }
        Body::Resolve {
            trial,
            chosen,
            target,
            hit,
            s_server,
            s_client,
            nonce,
        } => {
            // The resolve body carries no account, so it is taken from the trial's commit row.
            // The `SELECT` is also the check that the commit exists: it inserts nothing when it
            // does not, which is why the row count is inspected rather than trusted.
            let written = tx.execute(
                "INSERT INTO log_entry
                   (seq, kind, trial_id, account_id, at, prev_hash, entry_hash,
                    chosen, target, hit, s_server, s_client, nonce)
                 SELECT ?1, 'resolve', ?2, account_id, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11
                   FROM log_entry WHERE trial_id = ?2 AND kind = 'commit'",
                params![
                    entry.seq,
                    trial,
                    entry.at,
                    entry.prev,
                    entry.hash,
                    chosen,
                    target,
                    *hit as i64,
                    s_server,
                    s_client,
                    nonce
                ],
            )?;
            if written != 1 {
                return Err(DbError::OrphanResolve {
                    trial: trial.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Rebuilds an [`Entry`] from a row selected with [`ENTRY_COLUMNS`].
///
/// The outer `Result` is rusqlite's, so this can be used with `query_map`; the inner one carries
/// a row this reader does not understand, which is a schema newer than the binary.
fn entry_from_row(row: &Row<'_>) -> rusqlite::Result<Result<Entry, DbError>> {
    let seq: u64 = row.get(0)?;
    let kind: String = row.get(1)?;
    let trial: String = row.get(2)?;
    let account: String = row.get(3)?;
    let at: String = row.get(4)?;
    let prev: String = row.get(5)?;
    let hash: String = row.get(6)?;

    let body = match kind.as_str() {
        "commit" => Body::Commit {
            trial,
            account,
            coordinate: row.get(7)?,
            commitment: row.get(8)?,
            pool_version: row.get(9)?,
            pool_manifest_hash: row.get(16)?,
        },
        "resolve" => Body::Resolve {
            trial,
            chosen: row.get(10)?,
            target: row.get(11)?,
            hit: row.get::<_, i64>(12)? != 0,
            s_server: row.get(13)?,
            s_client: row.get(14)?,
            nonce: row.get(15)?,
        },
        other => {
            return Ok(Err(DbError::UnknownEntryKind {
                seq,
                kind: other.to_string(),
            }));
        }
    };

    Ok(Ok(Entry {
        seq,
        at,
        body,
        prev,
        hash,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A file in the temp directory, removed with its WAL siblings when the test ends. Written by
    /// hand rather than with a crate: the dependency would exist for six lines.
    struct TempDb(PathBuf);

    impl TempDb {
        fn new(tag: &str) -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let pid = std::process::id();
            let mut p = std::env::temp_dir();
            p.push(format!("vriltrainer-{tag}-{pid}-{n}.db"));
            let _ = std::fs::remove_file(&p);
            TempDb(p)
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
            }
        }
    }

    fn account(db: &Db, id: &str) {
        let w = lock(&db.writer);
        w.execute(
            "INSERT INTO account (id, public_id, token_hash, created_at)
             VALUES (?1, ?1, ?1, ?2)",
            params![id, now_rfc3339()],
        )
        .unwrap();
    }

    fn commit(trial: &str) -> Body {
        Body::Commit {
            trial: trial.into(),
            account: "acct".into(),
            coordinate: "4821-9037".into(),
            commitment: "sha256:aa".into(),
            pool_version: 1,
            pool_manifest_hash: Some("sha256:pool".into()),
        }
    }

    fn resolve(trial: &str, hit: bool) -> Body {
        Body::Resolve {
            trial: trial.into(),
            chosen: "img_1".into(),
            target: "img_2".into(),
            hit,
            s_server: "aa".into(),
            s_client: "bb".into(),
            nonce: "cc".into(),
        }
    }

    #[test]
    fn a_fresh_database_has_a_genesis_head() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.head().unwrap(), (0, GENESIS.to_string()));
        assert_eq!(db.verify_chain().unwrap(), 0);
    }

    #[test]
    fn migrations_run_once_and_are_idempotent() {
        let tmp = TempDb::new("migrate");
        Db::open(&tmp.0).unwrap();
        let db = Db::open(&tmp.0).unwrap();
        let r = db.reader().unwrap();
        let applied: u32 = r
            .query_row("SELECT COUNT(*) FROM schema_version", [], |x| x.get(0))
            .unwrap();
        assert_eq!(applied, MIGRATIONS.len() as u32);
    }

    /// The shape of the append path: the second entry names the first as its predecessor, and the
    /// chain walks.
    #[test]
    fn appending_twice_yields_linked_entries() {
        let db = Db::open_in_memory().unwrap();
        account(&db, "acct");

        let first = db.append(&now_rfc3339(), commit("t1")).unwrap();
        let second = db.append(&now_rfc3339(), commit("t2")).unwrap();

        assert_eq!(first.seq, 1);
        assert_eq!(first.prev, GENESIS);
        assert_eq!(second.seq, 2);
        assert_eq!(
            second.prev, first.hash,
            "the second entry must name the first"
        );
        assert_eq!(db.head().unwrap(), (2, second.hash.clone()));
        assert_eq!(db.verify_chain().unwrap(), 2);
    }

    #[test]
    fn a_resolve_inherits_the_account_from_its_commit() {
        let db = Db::open_in_memory().unwrap();
        account(&db, "acct");
        db.append(&now_rfc3339(), commit("t1")).unwrap();
        db.append(&now_rfc3339(), resolve("t1", true)).unwrap();

        let r = db.reader().unwrap();
        let owner: String = r
            .query_row(
                "SELECT account_id FROM log_entry WHERE kind = 'resolve'",
                [],
                |x| x.get(0),
            )
            .unwrap();
        assert_eq!(owner, "acct");
    }

    /// The pool binding of D34 survives the round trip through SQLite, which is the only way a
    /// reader ever sees it: the export is built from rows, not from what the append path held.
    #[test]
    fn a_commit_carries_its_pool_manifest_hash_back_out_of_the_database() {
        let db = Db::open_in_memory().unwrap();
        account(&db, "acct");
        let written = db.append(&now_rfc3339(), commit("t1")).unwrap();

        let read = db.entries_from(1, 10).unwrap();
        assert_eq!(read[0].body, written.body);
        match &read[0].body {
            Body::Commit {
                pool_manifest_hash, ..
            } => assert_eq!(pool_manifest_hash.as_deref(), Some("sha256:pool")),
            other => panic!("expected a commit, got {other:?}"),
        }
        assert_eq!(db.verify_chain().unwrap(), 1);
    }

    /// Migration 2 meets a log that already exists, which is the only way it will ever run in
    /// production. The rows from before it are not rewritten to carry the new field — that would
    /// change what they hash to — so the test that matters is that the chain still walks across
    /// the change and that the log carries on from where it was (D34).
    #[test]
    fn a_log_written_before_the_pool_binding_still_walks_after_it() {
        let tmp = TempDb::new("pool-binding");

        // The database as migration 1 left it, with one commit in the shape of that day.
        let legacy = Body::Commit {
            trial: "t1".into(),
            account: "acct".into(),
            coordinate: "4821-9037".into(),
            commitment: "sha256:aa".into(),
            pool_version: 1,
            pool_manifest_hash: None,
        };
        let at = now_rfc3339();
        let hash = entry_hash(GENESIS, 1, &at, &legacy);
        {
            let conn = Connection::open(&tmp.0).unwrap();
            conn.execute_batch(MIGRATIONS[0].1).unwrap();
            // The bookkeeping table belongs to the migration runner rather than to any migration,
            // so it is created here the same way `migrate` creates it.
            conn.execute_batch(
                "CREATE TABLE schema_version (
                     version    INTEGER NOT NULL PRIMARY KEY,
                     applied_at TEXT    NOT NULL
                 )",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO schema_version (version, applied_at) VALUES (1, ?1)",
                params![at],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO account (id, public_id, token_hash, created_at)
                 VALUES ('acct', 'acct', 'acct', ?1)",
                params![at],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO log_entry
                   (seq, kind, trial_id, account_id, at, prev_hash, entry_hash,
                    coordinate, commitment, pool_version)
                 VALUES (1, 'commit', 't1', 'acct', ?1, ?2, ?3, '4821-9037', 'sha256:aa', 1)",
                params![at, GENESIS, hash],
            )
            .unwrap();
        }

        let db = Db::open(&tmp.0).unwrap();
        assert_eq!(
            db.verify_chain().unwrap(),
            1,
            "the old entry still verifies"
        );

        let read = db.entries_from(1, 10).unwrap();
        assert_eq!(read[0].body, legacy);
        assert_eq!(read[0].hash, hash, "the entry was not rewritten");

        // And the log carries on, now binding: the new entry names its predecessor as usual.
        let next = db.append(&now_rfc3339(), commit("t2")).unwrap();
        assert_eq!(next.prev, hash);
        assert_eq!(db.verify_chain().unwrap(), 2);
    }

    /// The store-side half of [`ChainError::PoolBindingDropped`]. The column is nullable so that
    /// rows written before migration 2 keep the shape they hashed under; this is what stops that
    /// nullability from becoming a way to write a fresh trial that names no manifest.
    #[test]
    fn a_commit_cannot_drop_the_pool_binding_once_the_log_has_it() {
        let db = Db::open_in_memory().unwrap();
        account(&db, "acct");
        db.append(&now_rfc3339(), commit("t1")).unwrap();

        let unbound = match commit("t2") {
            Body::Commit {
                trial,
                account,
                coordinate,
                commitment,
                pool_version,
                ..
            } => Body::Commit {
                trial,
                account,
                coordinate,
                commitment,
                pool_version,
                pool_manifest_hash: None,
            },
            other => other,
        };
        assert!(db.append(&now_rfc3339(), unbound).is_err());
        assert_eq!(db.verify_chain().unwrap(), 1);
    }

    /// FR-037's replay defence, expressed as a constraint rather than a check that can be
    /// forgotten: a second evaluated answer for the same trial cannot be written at all.
    #[test]
    fn a_trial_cannot_be_resolved_twice() {
        let db = Db::open_in_memory().unwrap();
        account(&db, "acct");
        db.append(&now_rfc3339(), commit("t1")).unwrap();
        db.append(&now_rfc3339(), resolve("t1", false)).unwrap();
        assert!(db.append(&now_rfc3339(), resolve("t1", true)).is_err());
    }

    /// `also` runs in the same transaction, so a failure there leaves no entry behind. An audit
    /// log with a record of something that did not happen is worse than one missing a record.
    #[test]
    fn a_failure_alongside_the_append_rolls_the_entry_back() {
        let db = Db::open_in_memory().unwrap();
        account(&db, "acct");
        let outcome = db.append_with(&now_rfc3339(), commit("t1"), |_, _| {
            Err::<(), _>(DbError::Migration {
                version: 0,
                message: "deliberate".into(),
            })
        });
        assert!(outcome.is_err());
        assert_eq!(db.head().unwrap().0, 0, "nothing may survive the rollback");
    }

    #[test]
    fn entries_come_back_in_sequence_order() {
        let db = Db::open_in_memory().unwrap();
        account(&db, "acct");
        for i in 0..5 {
            db.append(&now_rfc3339(), commit(&format!("t{i}"))).unwrap();
        }
        let seqs: Vec<u64> = db
            .entries_from(2, 10)
            .unwrap()
            .iter()
            .map(|e| e.seq)
            .collect();
        assert_eq!(seqs, vec![2, 3, 4, 5]);
    }

    #[test]
    fn verify_chain_catches_a_doctored_row() {
        let tmp = TempDb::new("doctored");
        let db = Db::open(&tmp.0).unwrap();
        account(&db, "acct");
        db.append(&now_rfc3339(), commit("t1")).unwrap();
        db.append(&now_rfc3339(), commit("t2")).unwrap();
        assert_eq!(db.verify_chain().unwrap(), 2);

        // What an operator rewriting history does: change a field and leave the hashes alone.
        // Neither UNIQUE constraint notices — only the walk does.
        {
            let w = lock(&db.writer);
            w.execute(
                "UPDATE log_entry SET coordinate = '0000-0000' WHERE seq = 2",
                [],
            )
            .unwrap();
        }
        assert!(matches!(
            db.verify_chain(),
            Err(DbError::Chain(ChainError::HashMismatch { seq: 2 }))
        ));
    }

    #[test]
    fn verify_chain_catches_a_deleted_entry() {
        let tmp = TempDb::new("deleted");
        let db = Db::open(&tmp.0).unwrap();
        account(&db, "acct");
        for i in 0..3 {
            db.append(&now_rfc3339(), commit(&format!("t{i}"))).unwrap();
        }
        {
            let w = lock(&db.writer);
            w.execute("DELETE FROM log_entry WHERE seq = 2", [])
                .unwrap();
        }
        assert!(matches!(
            db.verify_chain(),
            Err(DbError::Chain(ChainError::SequenceBroken {
                at: 3,
                expected: 2
            }))
        ));
    }

    /// The D24 case, as close as one process gets to it: two independent `Db` handles are two
    /// independent SQLite connections, sharing nothing but the file.
    ///
    /// This test is the reason `append_with` says `IMMEDIATE`, and it has been checked against the
    /// alternatives. With `Deferred` it fails inside a second — the loser reads the head, then
    /// tries to upgrade a snapshot the winner has already moved past, and gets
    /// `SQLITE_BUSY_SNAPSHOT`, which `busy_timeout` deliberately does not retry. With the head
    /// read outside a transaction altogether it would not fail here at all: both writers would
    /// build entries naming the same predecessor, and only the `UNIQUE` on `prev_hash` would stop
    /// the second one reaching the file.
    #[test]
    fn two_writers_against_one_file_do_not_fork_the_chain() {
        let tmp = TempDb::new("two-writers");
        let first = Arc::new(Db::open(&tmp.0).unwrap());
        account(&first, "acct");
        let second = Arc::new(Db::open(&tmp.0).unwrap());

        let rounds = 25;
        let handles: Vec<_> = [first.clone(), second.clone()]
            .into_iter()
            .enumerate()
            .map(|(w, db)| {
                std::thread::spawn(move || {
                    for i in 0..rounds {
                        db.append(&now_rfc3339(), commit(&format!("w{w}-t{i}")))
                            .unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(first.verify_chain().unwrap(), 2 * rounds);
        assert_eq!(first.head().unwrap().0, 2 * rounds);
    }
}
