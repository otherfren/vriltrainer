//! The key to the public admin API (D25).
//!
//! One key, and only its hash is stored — the same discipline D9 applies to a player's access
//! token, for the same reason: a stolen database backup must carry no usable credential.
//!
//! The hash lives in the database rather than in an environment file precisely so that rotating it
//! needs no restart. A rotation that costs downtime is a rotation that never happens, and this is
//! the only privileged surface in the system.
//!
//! **None of this is what bounds a leaked key.** That is D25's other half: every operation the
//! admin API performs is reversible — approve a name, reject a name, nothing else. Deleting an
//! account, touching the log, changing pool versions all stay CLI subcommands behind SSH. A key
//! that leaks therefore costs an embarrassing name on the board for an hour, which is why this
//! module has no roles, no scopes and no expiry: there is one privilege level and it cannot do
//! damage.

use rand::RngCore;
use rusqlite::params;

use crate::db::{Db, DbError};
use crate::framing::framed_hex;

/// Bytes of key. The same 256 bits as a player token: it is pasted by a human once and then lives
/// in a reviewer's password manager, so the only cost of the extra length is that one paste.
const KEY_BYTES: usize = 32;

/// Bytes of key identifier. Names a row so a decision can be attributed to the key that made it;
/// never presented by a caller, so it does not have to be unguessable.
const ID_BYTES: usize = 8;

/// The result of a rotation. `key` exists here and in nothing else the process will ever produce
/// again — the database holds a hash.
pub struct Rotation {
    pub id: String,
    /// Printed once by the CLI and then dropped. There is no second chance to read it.
    pub key: String,
    /// How many keys this rotation retired. Zero on a fresh database; one on every later run,
    /// and anything else means somebody inserted a row by hand.
    pub revoked: usize,
}

/// At least 128 bits from a CSPRNG, as D9 requires of every bearer credential here.
pub fn mint() -> String {
    random_hex(KEY_BYTES)
}

/// Framed like every other hash in the repository ([`crate::framing`]), so there is one hashing
/// rule to reason about rather than a second one that only this table uses.
pub fn key_hash(key: &str) -> String {
    framed_hex(&[key.as_bytes()])
}

/// Issues a new key and retires every key that came before it, returning the new one **once**.
///
/// Both halves happen in one transaction. Revoking and inserting as two commits leaves a window in
/// which a crash has taken the reviewers' key away and put nothing in its place — recoverable only
/// by another rotation, which is exactly the situation an operator would be in the middle of.
pub fn rotate(db: &Db, label: &str, now: &str) -> Result<Rotation, DbError> {
    let key = mint();
    let id = random_hex(ID_BYTES);
    let hash = key_hash(&key);

    let revoked = db.write(|tx| {
        // Retired rather than deleted, so `last_used_at` on the old row still answers "was the key
        // I am replacing ever actually used", and so a rotation is itself a reversible-looking
        // record rather than a hole.
        let revoked = tx.execute(
            "UPDATE admin_key SET revoked_at = ?1 WHERE revoked_at IS NULL",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO admin_key (id, label, hash, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, label, hash, now],
        )?;
        Ok(revoked)
    })?;

    Ok(Rotation { id, key, revoked })
}

/// The identifier of the active key `presented` is, if it is one.
///
/// Every active key is read and every one is compared, rather than letting SQLite match the hash
/// with an indexed `WHERE hash = ?1`. Two things follow from that and both are deliberate: the
/// comparison is [`constant_time_eq`], and the loop does not stop at the row that matched.
pub fn authenticate(db: &Db, presented: &str, now: &str) -> Result<Option<String>, DbError> {
    let presented = key_hash(presented);

    // Scoped, because an in-memory database serves reads from the writer connection and a reader
    // still held when the `last_used_at` write below opens its transaction would deadlock.
    let found = {
        let r = db.reader()?;
        let mut stmt = r.prepare("SELECT id, hash FROM admin_key WHERE revoked_at IS NULL")?;
        let rows = stmt.query_map([], |x| Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?)))?;

        let mut found = None;
        for row in rows {
            let (id, hash) = row?;
            if constant_time_eq(hash.as_bytes(), presented.as_bytes()) {
                // No `break`. There is one active key after any rotation, so the loop runs once
                // either way; leaving early would time the *position* of the match if that ever
                // stopped being true.
                found = Some(id);
            }
        }
        found
    };

    if let Some(id) = &found {
        // Only on success, and only so an operator can tell a key that is in use from one nobody
        // ever installed before rotating it away. Admin traffic is a handful of requests a day, so
        // taking the write lock here costs nothing that the trial path will notice.
        db.write(|tx| {
            tx.execute(
                "UPDATE admin_key SET last_used_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            Ok(())
        })?;
    }
    Ok(found)
}

/// Equality that does not return early.
///
/// Both operands are SHA-256 digests, so an early-exit comparison would leak the stored *digest*
/// and not the key, and a digest is preimage-resistant — there is no attack here to prevent today.
/// The rule is kept unconditional anyway: the next person comparing a credential in this file
/// should not first have to work out whether this particular one happened to be safe to `memcmp`.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // Lengths are public: both sides are a fixed-width framed digest, and a mismatch here means
    // the column was written by something other than `key_hash`.
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0u8;
    for (x, y) in a.iter().zip(b) {
        difference |= x ^ y;
    }
    // `black_box` so the accumulator cannot be optimised into a short-circuiting compare.
    std::hint::black_box(difference) == 0
}

fn random_hex(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-07-25T10:00:00Z";
    const LATER: &str = "2026-07-26T10:00:00Z";

    #[test]
    fn a_key_is_at_least_128_bits_and_never_repeats() {
        let a = mint();
        assert_eq!(a.len(), KEY_BYTES * 2, "hex of {KEY_BYTES} bytes");
        assert_ne!(a, mint());
    }

    /// The property the whole of this module rests on, and the same one
    /// `account::the_token_itself_never_reaches_the_database` asserts for player tokens.
    #[test]
    fn the_key_itself_never_reaches_the_database() {
        let db = Db::open_in_memory().unwrap();
        let issued = rotate(&db, "operator", NOW).unwrap();

        let r = db.reader().unwrap();
        let mut stmt = r.prepare("SELECT * FROM admin_key").unwrap();
        let columns = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();
        for i in 0..columns {
            if let rusqlite::types::Value::Text(text) = row.get(i).unwrap() {
                assert_ne!(text, issued.key, "column {i} holds the key in clear");
            }
        }
    }

    #[test]
    fn a_key_authenticates_itself_and_nothing_else() {
        let db = Db::open_in_memory().unwrap();
        let issued = rotate(&db, "operator", NOW).unwrap();

        assert_eq!(
            authenticate(&db, &issued.key, NOW).unwrap(),
            Some(issued.id)
        );
        assert_eq!(authenticate(&db, "not a key", NOW).unwrap(), None);
        assert_eq!(authenticate(&db, "", NOW).unwrap(), None);
    }

    /// D25's reason for putting the hash in the database at all: the replacement takes effect on
    /// the next request, from the same process, with nothing restarted.
    #[test]
    fn rotating_retires_the_previous_key_immediately() {
        let db = Db::open_in_memory().unwrap();
        let first = rotate(&db, "operator", NOW).unwrap();
        assert_eq!(first.revoked, 0, "nothing to retire on a fresh database");
        assert!(authenticate(&db, &first.key, NOW).unwrap().is_some());

        let second = rotate(&db, "reviewer", LATER).unwrap();
        assert_eq!(second.revoked, 1);
        assert_eq!(
            authenticate(&db, &first.key, LATER).unwrap(),
            None,
            "the retired key must stop opening the door"
        );
        assert!(authenticate(&db, &second.key, LATER).unwrap().is_some());
        assert_ne!(first.key, second.key);
    }

    /// So that an operator can tell a key somebody installed from one nobody ever did, before
    /// rotating it away.
    #[test]
    fn a_successful_authentication_is_dated_and_a_failed_one_is_not() {
        let db = Db::open_in_memory().unwrap();
        let issued = rotate(&db, "operator", NOW).unwrap();

        let last_used = |db: &Db| -> Option<String> {
            let r = db.reader().unwrap();
            r.query_row(
                "SELECT last_used_at FROM admin_key WHERE id = ?1",
                params![issued.id],
                |x| x.get(0),
            )
            .unwrap()
        };

        assert_eq!(last_used(&db), None);
        authenticate(&db, "wrong", NOW).unwrap();
        assert_eq!(last_used(&db), None, "a guess must not date the key");
        authenticate(&db, &issued.key, LATER).unwrap();
        assert_eq!(last_used(&db), Some(LATER.to_string()));
    }

    #[test]
    fn equality_holds_for_the_digests_it_compares() {
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(
            &key_hash("a").into_bytes(),
            &key_hash("a").into_bytes()
        ));
        assert!(!constant_time_eq(
            &key_hash("a").into_bytes(),
            &key_hash("b").into_bytes()
        ));
        // Differences at either end, since an accumulator that dropped one would still pass a
        // test that only ever changed the first byte.
        assert!(!constant_time_eq(b"xbcd", b"abcd"));
        assert!(!constant_time_eq(b"abcx", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abcde"));
    }
}
