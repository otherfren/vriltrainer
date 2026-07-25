//! Single-use handoff codes for the language switch (D11, FR-031).
//!
//! `vriltrainer.de` and `vriltrainer.com` are separate origins, so local storage does not travel
//! with the user and a naive switch arrives as an anonymous first-time visitor — losing the
//! progress and creating a duplicate account, which would put one person into the leaderboard and
//! the aggregate twice.
//!
//! The obvious fix, putting the long-lived token into the target URL, is the one thing this must
//! not do: it would place the secret in the address bar and in the target domain's history, and
//! streaming is a stated use case. Hence a code that is worth thirty seconds and one redemption.
//!
//! The two domains share one database (D24), so redemption is a lookup and no traffic passes
//! between the processes.

use rand::RngCore;
use rusqlite::{OptionalExtension, params};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::db::{Db, DbError};
use crate::framing::framed_hex;

/// How long a code is worth anything.
pub const LIFETIME_SECONDS: i64 = 30;

/// Bytes of code. It opens a whole account, so it is sized like the credential it stands in for
/// and not like a confirmation code — the thirty-second window is a second lock, not the only one.
const CODE_BYTES: usize = 16;

/// Mints a code for `account_id`. Only its hash is stored, the same discipline as the access
/// token: a code is a bearer credential for the whole account, briefly.
pub fn mint(db: &Db, account_id: &str, now: &str) -> Result<String, DbError> {
    let code = random_hex(CODE_BYTES);
    let expires_at =
        shifted(now, LIFETIME_SECONDS).expect("a timestamp this process just formatted parses");

    db.write(|tx| {
        // Dead codes are swept here rather than by a scheduled job. Every row names an account, so
        // a table that only grows is a per-person record kept for ever — which is what D28
        // refuses — and a code that expired thirty seconds ago is worth nothing to anybody. The
        // scan is over a table whose rows all die within half a minute.
        tx.execute(
            "DELETE FROM handoff_code WHERE expires_at <= ?1",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO handoff_code (code_hash, account_id, expires_at) VALUES (?1, ?2, ?3)",
            params![code_hash(&code), account_id, expires_at],
        )?;
        Ok(())
    })?;
    Ok(code)
}

/// Burns the code and returns a fresh access token for the account it belonged to.
///
/// Single use is enforced by the burn happening in the same transaction as the lookup, or two
/// concurrent redemptions both succeed.
///
/// **The token is a new one, and the old one stops working.** That is forced rather than chosen:
/// only the hash of an access token is ever stored (D9), so the token the user already holds
/// cannot be handed back — and storing it in clear so that it could be would put a usable
/// credential into every backup, which is the property D9 exists to have. The consequence is that
/// the origin domain's copy dies the moment the switch is redeemed. Correct, too: the user is on
/// the other domain now, and one account holding one live token is what the schema says.
pub fn redeem(db: &Db, code: &str, now: &str) -> Result<Option<String>, DbError> {
    db.write(|tx| {
        // Burn and lookup are one statement. Two of them, however close together, let two
        // redemptions of the same code both find it unused; the `used_at IS NULL` predicate is
        // what makes exactly one of them update a row.
        let account_id: Option<String> = tx
            .query_row(
                "UPDATE handoff_code SET used_at = ?2
                  WHERE code_hash = ?1 AND used_at IS NULL AND expires_at > ?2
              RETURNING account_id",
                params![code_hash(code), now],
                |r| r.get(0),
            )
            .optional()?;

        let Some(account_id) = account_id else {
            return Ok(None);
        };

        let token = super::mint_token();
        tx.execute(
            "UPDATE account SET token_hash = ?2 WHERE id = ?1",
            params![account_id, super::token_hash(&token)],
        )?;
        Ok(Some(token))
    })
}

/// Framed like every other hash here ([`crate::framing`]).
fn code_hash(code: &str) -> String {
    framed_hex(&[code.as_bytes()])
}

/// A log timestamp shifted by whole seconds, in the format the column holds — so the comparisons
/// above stay string comparisons, which RFC 3339 in UTC makes correct.
fn shifted(at: &str, seconds: i64) -> Option<String> {
    (OffsetDateTime::parse(at, &Rfc3339).ok()? + Duration::seconds(seconds))
        .format(&Rfc3339)
        .ok()
}

fn random_hex(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account;

    const NOW: &str = "2026-07-25T10:00:00Z";
    const WITHIN: &str = "2026-07-25T10:00:29Z";
    const AFTER: &str = "2026-07-25T10:00:31Z";

    fn account_of(db: &Db) -> String {
        account::create(db, "otherfren", NOW)
            .expect("the fixture name passes the filter")
            .id
    }

    /// SC-007 at the level this module owns: the code resolves to the account that minted it, and
    /// the token it hands over opens that same account and no other.
    #[test]
    fn a_code_carries_the_account_across_and_creates_nothing_new() {
        let db = Db::open_in_memory().unwrap();
        let id = account_of(&db);

        let code = mint(&db, &id, NOW).unwrap();
        let token = redeem(&db, &code, WITHIN).unwrap().expect("a live code");

        assert_eq!(account::authenticate(&db, &token).unwrap(), Some(id));
        let r = db.reader().unwrap();
        let accounts: u32 = r
            .query_row("SELECT COUNT(*) FROM account", [], |x| x.get(0))
            .unwrap();
        assert_eq!(accounts, 1, "a switch must not mint a second account");
    }

    /// The reason the old token dies: it cannot be returned, because it was never stored. Asserted
    /// rather than left implicit, so that "fixing" the logout by keeping the token in clear has to
    /// break a test that says why not.
    #[test]
    fn redemption_replaces_the_token_it_could_not_hand_back() {
        let db = Db::open_in_memory().unwrap();
        let created = account::create(&db, "otherfren", NOW).unwrap();

        let code = mint(&db, &created.id, NOW).unwrap();
        let token = redeem(&db, &code, NOW).unwrap().expect("a live code");

        assert_ne!(token, created.access_token);
        assert_eq!(
            account::authenticate(&db, &created.access_token).unwrap(),
            None,
            "the origin domain's copy is retired by the switch"
        );
        assert_eq!(
            account::authenticate(&db, &token).unwrap(),
            Some(created.id)
        );
    }

    #[test]
    fn a_code_is_worth_exactly_one_redemption() {
        let db = Db::open_in_memory().unwrap();
        let id = account_of(&db);
        let code = mint(&db, &id, NOW).unwrap();

        assert!(redeem(&db, &code, NOW).unwrap().is_some());
        assert!(
            redeem(&db, &code, NOW).unwrap().is_none(),
            "a burnt code must open nothing"
        );
    }

    #[test]
    fn a_code_expires_and_an_unknown_one_was_never_anything() {
        let db = Db::open_in_memory().unwrap();
        let id = account_of(&db);
        let code = mint(&db, &id, NOW).unwrap();

        assert!(redeem(&db, &code, AFTER).unwrap().is_none());
        assert!(redeem(&db, "not a code", NOW).unwrap().is_none());
        // A refused redemption does not burn the code — the guard fails before `used_at` is
        // written — so the window is what closed it, and nothing else.
        assert!(redeem(&db, &code, WITHIN).unwrap().is_some());
    }

    /// The codes belong to whoever minted them, and one account's code does not open another's.
    #[test]
    fn codes_are_not_interchangeable_between_accounts() {
        let db = Db::open_in_memory().unwrap();
        let first = account_of(&db);
        let second = account_of(&db);
        assert_ne!(first, second);

        let code = mint(&db, &first, NOW).unwrap();
        let token = redeem(&db, &code, NOW).unwrap().unwrap();
        assert_eq!(account::authenticate(&db, &token).unwrap(), Some(first));
    }

    /// Nothing survives its own lifetime, so the table cannot become a record of who switched
    /// language and when (D28).
    #[test]
    fn expired_codes_are_swept_by_the_next_mint() {
        let db = Db::open_in_memory().unwrap();
        let id = account_of(&db);

        mint(&db, &id, NOW).unwrap();
        mint(&db, &id, NOW).unwrap();
        assert_eq!(rows(&db), 2, "two live codes coexist");

        mint(&db, &id, AFTER).unwrap();
        assert_eq!(rows(&db), 1, "only the code just minted is left");
    }

    fn rows(db: &Db) -> u32 {
        let r = db.reader().unwrap();
        r.query_row("SELECT COUNT(*) FROM handoff_code", [], |x| x.get(0))
            .unwrap()
    }

    #[test]
    fn the_code_itself_never_reaches_the_database() {
        let db = Db::open_in_memory().unwrap();
        let id = account_of(&db);
        let code = mint(&db, &id, NOW).unwrap();

        let r = db.reader().unwrap();
        let stored: String = r
            .query_row("SELECT code_hash FROM handoff_code", [], |x| x.get(0))
            .unwrap();
        assert_ne!(stored, code);
        assert_eq!(stored, code_hash(&code));
    }
}
