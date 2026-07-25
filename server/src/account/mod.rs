//! Accounts and the capability token (D9).
//!
//! There is no registration, no email and no password. The user invents a name, the server issues
//! a secret access link, and that link is the account. Losing it loses the history, deliberately:
//! a recovery path is an authentication surface to maintain for a case the user was warned about
//! twice, and email recovery would pull real GDPR obligations onto a solo operator.

pub mod handoff;
pub mod name;
pub mod name_filter;

use rand::RngCore;
use rusqlite::{ErrorCode, OptionalExtension, params};

use crate::db::{Db, DbError};
use crate::framing::framed_hex;
use name::NameError;

/// Bytes of token. D9 requires at least 128 bits; 256 costs a longer fragment and nothing else,
/// and this is the only credential the account has.
const TOKEN_BYTES: usize = 32;

/// Bytes of opaque account id. This value appears in every log entry the account ever produces,
/// so it must be unguessable as well as unique — a sequential id would let anyone reading the
/// export count the accounts and order them by signup.
const ID_BYTES: usize = 16;

/// How many public identifiers to draw before giving up.
///
/// The identifier is six hex characters because that is what the client renders beside a name, so
/// the space is 2^24 and a birthday collision is a matter of *when*, not *if*. Uniqueness comes
/// from the `UNIQUE` constraint and this redraw rather than from the size of the space: with a few
/// thousand accounts the first draw collides once in a few thousand attempts, and eight
/// consecutive collisions is not a number this site reaches.
const PUBLIC_ID_ATTEMPTS: u32 = 8;

/// A freshly created account. The `access_token` field is the only place the token exists in full
/// — the database holds a hash — so this value is returned to the caller once and then dropped.
pub struct NewAccount {
    /// The opaque internal identifier. This, and never the name, is what appears in the log.
    pub id: String,
    /// Shown beside the name on public surfaces (FR-029). Drawn independently of the token:
    /// deriving it would publish a function of the secret.
    pub public_id: String,
    pub access_token: String,
    /// The name as stored, which is the normalised form and not necessarily what was typed. The
    /// response echoes this, or the client shows a name the server does not hold.
    pub name: String,
}

/// At least 128 bits from a CSPRNG (D9). Returned once and never again.
pub fn mint_token() -> String {
    random_hex(TOKEN_BYTES)
}

/// The token is a password and is treated as one: only this ever reaches the database.
///
/// Framed like every other hash in the repository ([`crate::framing`]). A bare SHA-256 would do
/// for a single fixed-length field, but a second hashing rule is a second rule to get right.
pub fn token_hash(token: &str) -> String {
    framed_hex(&[token.as_bytes()])
}

/// Creates an account with `name` in the `pending` state of D25 — nothing the user types is
/// public until a human has approved it.
///
/// The pre-filter runs here, not at the caller, so no route into the review queue can skip it.
pub fn create(db: &Db, name: &str, now: &str) -> Result<NewAccount, NameError> {
    let name = name_filter::normalise(name);
    name_filter::check(&name).map_err(NameError::Refused)?;

    let account = db.write(|tx| {
        let mut left = PUBLIC_ID_ATTEMPTS;
        loop {
            left -= 1;
            let account = NewAccount {
                id: random_hex(ID_BYTES),
                public_id: mint_public_id(),
                access_token: mint_token(),
                name: name.clone(),
            };
            // `name_changed_at` is set here because the first name is a submission like any other
            // — the rename cooldown of D25 starts at signup rather than handing every new account
            // one free extra name in the review queue.
            let written = tx.execute(
                "INSERT INTO account
                   (id, public_id, token_hash, display_name, name_state, name_changed_at,
                    created_at)
                 VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?5)",
                params![
                    account.id,
                    account.public_id,
                    token_hash(&account.access_token),
                    name,
                    now
                ],
            );
            match written {
                Ok(_) => return Ok(account),
                // Only the public identifier is small enough to collide; a repeated token hash or
                // account id would mean the CSPRNG has stopped being one, and that is not a
                // condition to paper over with a retry.
                Err(e) if left > 0 && is_taken(&e) => continue,
                Err(e) => return Err(DbError::from(e)),
            }
        }
    })?;
    Ok(account)
}

/// The account id a bearer token belongs to, if any.
///
/// A plain indexed lookup on the hash. There is nothing here for a timing attack to learn: the
/// comparison is against a 256-bit digest of the presented token, so distinguishing a near miss
/// from a miss still requires producing the digest, which requires the token.
pub fn authenticate(db: &Db, token: &str) -> Result<Option<String>, DbError> {
    let r = db.reader()?;
    let id = r
        .query_row(
            "SELECT id FROM account WHERE token_hash = ?1",
            params![token_hash(token)],
            |x| x.get(0),
        )
        .optional()?;
    Ok(id)
}

/// Removes the display name, satisfying erasure (FR-035). The account's trials stay in the log
/// under its opaque identifier and every proof over them still verifies (FR-036) — which is the
/// entire reason names were kept out of the chain in the first place.
///
/// Permanent, and marked by `display_name` being null: every account is created with a name, so
/// that is a state no live account reaches by any other route, and it is what stops
/// [`name::submit`] from ever setting one again.
pub fn forget_name(db: &Db, account_id: &str, now: &str) -> Result<(), DbError> {
    db.write(|tx| {
        tx.execute(
            "UPDATE account
                SET display_name = NULL, public_name = NULL, name_state = 'rejected',
                    name_reason = ?2, name_changed_at = ?3
              WHERE id = ?1",
            params![account_id, name::ERASED, now],
        )?;
        Ok(())
    })
}

/// The public identifier: six uppercase hex characters, which is the form the client renders
/// beside a name.
fn mint_public_id() -> String {
    let mut b = [0u8; 3];
    rand::rng().fill_bytes(&mut b);
    format!("{:02X}{:02X}{:02X}", b[0], b[1], b[2])
}

fn random_hex(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

/// A `UNIQUE` violation, which on this insert can only be the public identifier.
fn is_taken(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if f.code == ErrorCode::ConstraintViolation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::chain::Body;

    const NOW: &str = "2026-07-25T10:00:00Z";
    const LATER: &str = "2026-07-26T10:00:01Z";

    fn column(db: &Db, id: &str, sql: &str) -> Option<String> {
        let r = db.reader().unwrap();
        r.query_row(sql, params![id], |x| x.get(0)).unwrap()
    }

    #[test]
    fn a_token_is_at_least_128_bits_and_never_repeats() {
        let a = mint_token();
        assert_eq!(a.len(), TOKEN_BYTES * 2, "hex of {TOKEN_BYTES} bytes");
        assert!(a.len() >= 32, "D9's floor is 128 bits");
        assert_ne!(a, mint_token());
    }

    /// The property the whole of D9 rests on: a database backup carries no usable credential.
    #[test]
    fn the_token_itself_never_reaches_the_database() {
        let db = Db::open_in_memory().unwrap();
        let account = create(&db, "otherfren", NOW).unwrap();

        {
            let r = db.reader().unwrap();
            let mut stmt = r.prepare("SELECT * FROM account").unwrap();
            let columns = stmt.column_count();
            let mut rows = stmt.query([]).unwrap();
            let row = rows.next().unwrap().unwrap();
            for i in 0..columns {
                let value: rusqlite::types::Value = row.get(i).unwrap();
                if let rusqlite::types::Value::Text(text) = value {
                    assert_ne!(text, account.access_token, "column {i} holds the token in clear");
                }
            }
        }
        assert_eq!(
            column(&db, &account.id, "SELECT token_hash FROM account WHERE id = ?1"),
            Some(token_hash(&account.access_token))
        );
    }

    #[test]
    fn a_token_authenticates_its_own_account_and_nothing_else() {
        let db = Db::open_in_memory().unwrap();
        let first = create(&db, "otherfren", NOW).unwrap();
        let second = create(&db, "Monroe Institut", NOW).unwrap();

        assert_eq!(authenticate(&db, &first.access_token).unwrap(), Some(first.id.clone()));
        assert_eq!(authenticate(&db, &second.access_token).unwrap(), Some(second.id));
        // No recovery path to hint at, so an unknown token is simply nobody (D9).
        assert_eq!(authenticate(&db, "not a token").unwrap(), None);
        assert_eq!(authenticate(&db, "").unwrap(), None);
    }

    /// FR-029. Six uppercase hex characters is what the leaderboard renders beside a name, and
    /// the `UNIQUE` constraint is what makes a repeat impossible rather than unlikely.
    #[test]
    fn public_identifiers_are_six_hex_characters_and_distinct() {
        let db = Db::open_in_memory().unwrap();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let account = create(&db, "otherfren", NOW).unwrap();
            assert_eq!(account.public_id.len(), 6);
            assert!(
                account.public_id.chars().all(|c| c.is_ascii_digit() || c.is_ascii_uppercase()),
                "{} is not uppercase hex",
                account.public_id
            );
            assert!(seen.insert(account.public_id), "a public identifier was reused");
        }
    }

    /// Names are not unique (FR-049); the public identifier is what tells two of them apart.
    #[test]
    fn two_accounts_may_share_a_name() {
        let db = Db::open_in_memory().unwrap();
        let first = create(&db, "otherfren", NOW).unwrap();
        let second = create(&db, "otherfren", NOW).unwrap();
        assert_ne!(first.public_id, second.public_id);
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn a_refused_name_creates_no_account() {
        let db = Db::open_in_memory().unwrap();
        assert!(matches!(create(&db, "h1tl3r", NOW), Err(NameError::Refused(_))));

        let r = db.reader().unwrap();
        let accounts: u32 = r.query_row("SELECT COUNT(*) FROM account", [], |x| x.get(0)).unwrap();
        assert_eq!(accounts, 0);
    }

    /// SC-008 and FR-036, which is the reason FR-026 keeps names out of the chain: erasure costs
    /// one row in `account` and nothing in the log.
    #[test]
    fn erasing_a_name_leaves_every_published_trial_verifiable() {
        let db = Db::open_in_memory().unwrap();
        let account = create(&db, "otherfren", NOW).unwrap();
        let commit = Body::Commit {
            trial: "t1".into(),
            account: account.id.clone(),
            coordinate: "4821-9037".into(),
            commitment: "sha256:aa".into(),
            pool_version: 1,
        };
        let entry = db.append(NOW, commit).unwrap();

        forget_name(&db, &account.id, LATER).unwrap();

        assert_eq!(db.verify_chain().unwrap(), 1);
        assert_eq!(db.entries_from(1, 10).unwrap(), vec![entry]);
        assert_eq!(column(&db, &account.id, "SELECT display_name FROM account WHERE id = ?1"), None);
        // Still playable, under the identifier the log already names.
        assert_eq!(authenticate(&db, &account.access_token).unwrap(), Some(account.id));
    }

    #[test]
    fn erasure_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let account = create(&db, "otherfren", NOW).unwrap();
        forget_name(&db, &account.id, LATER).unwrap();
        forget_name(&db, &account.id, LATER).unwrap();
        assert_eq!(name::holder(&db, &account.id).unwrap().name, None);
    }
}
