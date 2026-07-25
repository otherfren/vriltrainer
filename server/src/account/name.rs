//! The name state machine of D25: `pending` -> `approved` | `rejected`.
//!
//! Two audiences see different things, and that is the whole design. The holder always sees the
//! name they chose, in whatever state it is — they cannot pick a better one without seeing the one
//! that was refused. The public sees the most recently approved name and a fixed-length mask
//! otherwise, so a row reads as *a name exists here and has not been cleared* rather than as an
//! absence.
//!
//! **The only column a public surface may read is `public_name`**, and [`approve`] is its only
//! writer. `display_name` is the holder's copy and has no path to a page; a query that selects it
//! for the leaderboard is the one way the masking of FR-047 can be defeated, and it is visibly not
//! this module.
//!
//! FR-026 is untouched by any of this: the log references the opaque account id and never the
//! name, so nothing here reaches the record.

use rusqlite::params;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use super::name_filter::{self, Refusal};
use crate::db::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NameState {
    Pending,
    Approved,
    Rejected,
}

/// The mask shown in place of a name that has not been approved.
///
/// **Fixed length is the point.** A mask that preserved the real length and first letter would
/// still communicate the shape of a slur, which is precisely what pre-approval exists to keep off
/// the page. Same idiom as the masked access link in D9.
pub const MASK: &str = "••••••••";

/// How long a holder must wait between submissions (FR-048).
///
/// The scarce resource being rationed is the reviewer, not the database, so this is a cooldown per
/// account rather than a quota: one name in the queue per account per day is a queue a person can
/// still clear. A rejection clears the clock, because the user has not yet had a turn.
///
/// It runs from **submission** and not from the decision, which is what holds a name still while
/// it is being looked at. Were a pending name editable, a user could swap it between the moment
/// the reviewer read the queue and the moment they clicked approve, and a human would have
/// published a name nobody ever saw — the exact outcome pre-approval exists to prevent.
pub const RENAME_COOLDOWN_HOURS: i64 = 24;

/// The reason code left behind by erasure (FR-035). Written to `name_reason` so the client can say
/// why no name field is offered; the *state* is `display_name` being null, which is what
/// [`submit`] checks.
pub const ERASED: &str = "erased";

/// Why a name was not accepted.
#[derive(Debug, thiserror::Error)]
pub enum NameError {
    #[error(transparent)]
    Db(#[from] DbError),
    /// The pre-filter refused it ([`super::name_filter`]).
    #[error("the name was refused: {0:?}")]
    Refused(Refusal),
    /// A rename inside the cooldown of [`RENAME_COOLDOWN_HOURS`].
    #[error("renamed too recently, {retry_after_seconds}s remain")]
    TooSoon { retry_after_seconds: i64 },
    /// The account erased its name, and erasure is permanent (FR-035).
    #[error("the name was erased and cannot be set again")]
    Erased,
}

/// What the holder sees: their own name, its state, and why it was refused if it was.
///
/// `name` is `None` only for an erased account: every account is created with one, so null is a
/// state a live account cannot otherwise reach.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HolderView {
    pub name: Option<String>,
    pub state: NameState,
    /// A refusal code, not a sentence — the sentence is product copy and lives in the client.
    pub reason: Option<String>,
}

/// What everyone else sees on the leaderboard and in any shared artefact.
///
/// The public identifier is shown beside this either way (FR-029), so a masked row is still
/// attributable and still checkable against the log.
pub fn public_display(public_name: Option<&str>) -> String {
    match public_name {
        Some(n) => n.to_string(),
        None => MASK.to_string(),
    }
}

/// Records a chosen name and puts it in the review queue.
///
/// The pre-filter runs here rather than at the caller, so there is no route to the queue that
/// skips it. On a rename the previously **approved** name stays public until the new one clears,
/// so a rename is not punished with anonymity: this writes `display_name` and never touches
/// `public_name`.
pub fn submit(db: &Db, account_id: &str, name: &str, now: &str) -> Result<NameState, NameError> {
    let name = name_filter::normalise(name);
    name_filter::check(&name).map_err(NameError::Refused)?;

    // The refusals are decided inside the transaction: the cooldown is read-then-write, and two
    // requests racing it would each see an expired clock and both spend the turn.
    db.write(|tx| {
        let (current, changed_at): (Option<String>, Option<String>) = tx.query_row(
            "SELECT display_name, name_changed_at FROM account WHERE id = ?1",
            params![account_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        if current.is_none() {
            return Ok(Err(NameError::Erased));
        }
        if let Some(retry_after_seconds) = cooldown_remaining(changed_at.as_deref(), now) {
            return Ok(Err(NameError::TooSoon { retry_after_seconds }));
        }

        tx.execute(
            "UPDATE account
                SET display_name = ?2, name_state = 'pending', name_reason = NULL,
                    name_changed_at = ?3
              WHERE id = ?1",
            params![account_id, name, now],
        )?;
        Ok(Ok(NameState::Pending))
    })?
}

/// Publishes the name. Reversible, which is what allows this to sit behind a public admin API.
///
/// Reversible in both directions: a name [`reject`] pulled off the board goes back on if it turns
/// out to have been fine, so a rejected row is accepted here as readily as a pending one.
pub fn approve(db: &Db, account_id: &str) -> Result<(), DbError> {
    db.write(|tx| {
        // `display_name IS NOT NULL` keeps an erased account out. There is nothing to publish, and
        // publishing null would quietly re-mask an account that asked to be forgotten rather than
        // leaving it as it is.
        tx.execute(
            "UPDATE account
                SET public_name = display_name, name_state = 'approved', name_reason = NULL
              WHERE id = ?1 AND display_name IS NOT NULL",
            params![account_id],
        )?;
        Ok(())
    })
}

/// Refuses the name: it comes off the board, and the holder is told why.
///
/// `display_name` stays, because the holder has to see what was refused to pick something better;
/// it is discarded when they submit a replacement, and it reaches no public surface in the
/// meantime. `name_changed_at` is cleared, so a refusal does **not** consume the rename limit —
/// the user has not had a turn yet.
pub fn reject(db: &Db, account_id: &str, reason: &str) -> Result<(), DbError> {
    db.write(|tx| {
        tx.execute(
            "UPDATE account
                SET name_state = 'rejected', name_reason = ?2, public_name = NULL,
                    name_changed_at = NULL
              WHERE id = ?1 AND display_name IS NOT NULL",
            params![account_id, reason],
        )?;
        Ok(())
    })
}

/// The review queue, oldest submission first. It is `account` filtered to `pending`; a separate
/// table would be a second copy of a state that already exists.
///
/// Ordered by submission rather than by account age, so a rename queues behind the names already
/// waiting instead of jumping to the front of a queue it joined last.
pub fn pending(db: &Db, limit: u32) -> Result<Vec<(String, String)>, DbError> {
    let r = db.reader()?;
    let mut stmt = r.prepare(
        "SELECT id, display_name FROM account
          WHERE name_state = 'pending' AND display_name IS NOT NULL
          ORDER BY name_changed_at, created_at
          LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |x| Ok((x.get(0)?, x.get(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// What the holder is shown about their own name, whatever state it is in (FR-047).
pub fn holder(db: &Db, account_id: &str) -> Result<HolderView, DbError> {
    let r = db.reader()?;
    let (name, state, reason): (Option<String>, String, Option<String>) = r.query_row(
        "SELECT display_name, name_state, name_reason FROM account WHERE id = ?1",
        params![account_id],
        |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?)),
    )?;
    Ok(HolderView { name, state: state_from_column(&state), reason })
}

/// The column is constrained to these three values, so anything else is a row this binary did not
/// write. Read as unpublished rather than refused: a name shown to nobody is the safe reading of a
/// state nobody understands.
fn state_from_column(raw: &str) -> NameState {
    match raw {
        "approved" => NameState::Approved,
        "rejected" => NameState::Rejected,
        _ => NameState::Pending,
    }
}

/// Seconds left on the cooldown, or `None` if a name may be submitted now.
///
/// An unparsable stored timestamp counts as expired. This process writes the column with
/// `now_rfc3339`, so a value that will not parse means the row was edited by hand, and a hand edit
/// must not lock somebody out of their own name for ever.
fn cooldown_remaining(changed_at: Option<&str>, now: &str) -> Option<i64> {
    let last = OffsetDateTime::parse(changed_at?, &Rfc3339).ok()?;
    let now = OffsetDateTime::parse(now, &Rfc3339).ok()?;
    let allowed = last + Duration::hours(RENAME_COOLDOWN_HOURS);
    (allowed > now).then(|| (allowed - now).whole_seconds().max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account;

    const CREATED: &str = "2026-07-25T10:00:00Z";
    const NEXT_DAY: &str = "2026-07-26T10:00:01Z";
    const DAY_AFTER: &str = "2026-07-27T10:00:02Z";

    fn account_with_name(db: &Db, name: &str) -> String {
        account::create(db, name, CREATED).expect("the name passes the filter").id
    }

    /// Reads `public_name`, the only column a public surface is allowed to look at.
    fn on_the_board(db: &Db, account_id: &str) -> String {
        let r = db.reader().unwrap();
        let public: Option<String> = r
            .query_row("SELECT public_name FROM account WHERE id = ?1", params![account_id], |x| {
                x.get(0)
            })
            .unwrap();
        public_display(public.as_deref())
    }

    #[test]
    fn an_unapproved_name_masks_to_a_fixed_length() {
        // Two names of very different lengths must be indistinguishable once masked, or the mask
        // still leaks the shape of what it is hiding.
        assert_eq!(public_display(None), public_display(None));
        assert_eq!(public_display(Some("otherfren")), "otherfren");
    }

    /// The whole point of D25: nothing a user types is public until a human has said so.
    #[test]
    fn a_new_name_is_masked_until_it_is_approved() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        assert_eq!(on_the_board(&db, &id), MASK);
        approve(&db, &id).unwrap();
        assert_eq!(on_the_board(&db, &id), "otherfren");
    }

    /// FR-047: the holder sees their own name in whatever state it is, so they can pick a better
    /// one without guessing what was wrong with this one.
    #[test]
    fn the_holder_always_sees_their_own_name() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        let view = holder(&db, &id).unwrap();
        assert_eq!(view.name.as_deref(), Some("otherfren"));
        assert_eq!(view.state, NameState::Pending);

        reject(&db, &id, "hate").unwrap();
        let view = holder(&db, &id).unwrap();
        assert_eq!(view.name.as_deref(), Some("otherfren"), "a refused name is still its owner's");
        assert_eq!(view.state, NameState::Rejected);
        assert_eq!(view.reason.as_deref(), Some("hate"));
    }

    /// FR-048: renaming is not punished with anonymity. The board keeps the old name for as long
    /// as the new one is in the queue.
    #[test]
    fn the_last_approved_name_stays_up_while_a_rename_is_reviewed() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");
        approve(&db, &id).unwrap();

        submit(&db, &id, "ganzfeld_enjoyer", NEXT_DAY).unwrap();
        assert_eq!(on_the_board(&db, &id), "otherfren");
        assert_eq!(holder(&db, &id).unwrap().name.as_deref(), Some("ganzfeld_enjoyer"));

        approve(&db, &id).unwrap();
        assert_eq!(on_the_board(&db, &id), "ganzfeld_enjoyer");
    }

    #[test]
    fn a_rename_inside_the_cooldown_is_refused() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        let too_soon = submit(&db, &id, "Monroe Institut", CREATED);
        assert!(matches!(too_soon, Err(NameError::TooSoon { .. })));
        assert!(submit(&db, &id, "Monroe Institut", NEXT_DAY).is_ok());
    }

    /// D25 in one test: a refusal costs the user nothing but the name, or the rate limit would
    /// punish them for the reviewer's decision.
    #[test]
    fn a_rejection_does_not_consume_the_rename_limit() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        reject(&db, &id, "hate").unwrap();
        // The very instant that was refused on the cooldown a moment ago.
        assert!(submit(&db, &id, "Monroe Institut", CREATED).is_ok());
    }

    /// Reversible in both directions, which is what bounds the blast radius of the public admin
    /// API (D25).
    #[test]
    fn a_name_can_be_pulled_off_the_board_and_put_back() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        approve(&db, &id).unwrap();
        reject(&db, &id, "hate").unwrap();
        assert_eq!(on_the_board(&db, &id), MASK);

        approve(&db, &id).unwrap();
        assert_eq!(on_the_board(&db, &id), "otherfren");
    }

    #[test]
    fn the_queue_is_pending_names_in_submission_order() {
        let db = Db::open_in_memory().unwrap();
        let first = account_with_name(&db, "otherfren");
        let second = account_with_name(&db, "Monroe Institut");
        approve(&db, &first).unwrap();

        assert_eq!(
            pending(&db, 10).unwrap(),
            vec![(second.clone(), "Monroe Institut".to_string())],
            "an approved name has left the queue"
        );

        // The older account renames, and joins the queue behind the name already waiting.
        submit(&db, &first, "ganzfeld_enjoyer", NEXT_DAY).unwrap();
        let queue: Vec<String> = pending(&db, 10).unwrap().into_iter().map(|(id, _)| id).collect();
        assert_eq!(queue, vec![second, first]);
    }

    /// FR-035: erasure is permanent, and the account keeps playing under its opaque identifier.
    #[test]
    fn an_erased_name_can_never_be_set_again() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");
        approve(&db, &id).unwrap();

        account::forget_name(&db, &id, NEXT_DAY).unwrap();
        assert_eq!(on_the_board(&db, &id), MASK);

        let view = holder(&db, &id).unwrap();
        assert_eq!(view.name, None);
        assert_eq!(view.reason.as_deref(), Some(ERASED));

        // Not merely rate-limited: a day later, and after a rejection, it is still refused.
        assert!(matches!(submit(&db, &id, "Monroe Institut", DAY_AFTER), Err(NameError::Erased)));
        reject(&db, &id, "hate").unwrap();
        assert!(matches!(submit(&db, &id, "Monroe Institut", DAY_AFTER), Err(NameError::Erased)));
        assert!(pending(&db, 10).unwrap().is_empty());
    }

    /// A name the pre-filter refuses must not reach the queue through `submit`, or the filter is
    /// only a client-side courtesy after all.
    #[test]
    fn the_pre_filter_cannot_be_skipped_by_submitting_directly() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        let refused = submit(&db, &id, "h1tl3r", NEXT_DAY);
        assert!(matches!(refused, Err(NameError::Refused(Refusal::Hate))));
        assert_eq!(holder(&db, &id).unwrap().name.as_deref(), Some("otherfren"));
    }

    #[test]
    fn a_submitted_name_is_stored_normalised() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        submit(&db, &id, "  Monroe   Institut ", NEXT_DAY).unwrap();
        assert_eq!(holder(&db, &id).unwrap().name.as_deref(), Some("Monroe Institut"));
    }

    #[test]
    fn the_cooldown_counts_down_and_expires() {
        assert_eq!(cooldown_remaining(None, CREATED), None);
        assert_eq!(cooldown_remaining(Some(CREATED), CREATED), Some(24 * 3600));
        assert_eq!(cooldown_remaining(Some(CREATED), "2026-07-25T22:00:00Z"), Some(12 * 3600));
        assert_eq!(cooldown_remaining(Some(CREATED), NEXT_DAY), None);
        assert_eq!(cooldown_remaining(Some("not a timestamp"), CREATED), None);
    }
}
