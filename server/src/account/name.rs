//! The name state machine of D25: `pending` -> `approved` | `rejected`.
//!
//! Two audiences see different things, and that is the whole design. The holder always sees the
//! name they chose, in whatever state it is — they cannot pick a better one without seeing the one
//! that was refused. The public sees the most recently approved name and a fixed-length mask
//! otherwise, so a row reads as *a name exists here and has not been cleared* rather than as an
//! absence.
//!
//! **The only column a public surface may read is `public_name`**, and [`approve`] is the only
//! thing that ever puts a string into it — [`reject`] and [`erase`] can only clear it.
//! `display_name` is the holder's copy and has no path to a page; a query that selects it for the
//! leaderboard is the one way the masking of FR-047 can be defeated, and it is visibly not this
//! module.
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
    /// Permanent (FR-035). Distinct from `Rejected` because both now leave `display_name` null —
    /// a refusal discards the name too (SC-018) — so nullness can no longer tell them apart, and
    /// confusing them would lock a rejected holder out of ever choosing again.
    Erased,
}

/// The mask shown in place of a name that has not been approved.
///
/// **Fixed length is the point.** A mask that preserved the real length and first letter would
/// still communicate the shape of a slur, which is precisely what pre-approval exists to keep off
/// the page. Same idiom as the masked access link in D9.
pub const MASK: &str = "••••••••";

/// The reason code left behind by erasure (FR-035). Written to `name_reason` so the client can say
/// why no name field is offered; the *state* is `name_state = 'erased'`, which is what [`submit`]
/// checks.
pub const ERASED: &str = "erased";

/// Why a name was not accepted.
#[derive(Debug, thiserror::Error)]
pub enum NameError {
    #[error(transparent)]
    Db(#[from] DbError),
    /// The pre-filter refused it ([`super::name_filter`]).
    #[error("the name was refused: {0:?}")]
    Refused(Refusal),
    /// A rename inside [`crate::config::Config::rename_cooldown_hours`].
    #[error("renamed too recently, {retry_after_seconds}s remain")]
    TooSoon { retry_after_seconds: i64 },
    /// The account erased its name, and erasure is permanent (FR-035).
    #[error("the name was erased and cannot be set again")]
    Erased,
}

/// What the holder sees: their own name, its state, and why it was refused if it was.
///
/// `name` is `None` for an erased account and for one whose name was refused — a refusal discards
/// the string (SC-018) and leaves only `reason`, which is what the holder needs in order to choose
/// again.
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

/// The one door into the review queue: normalise, then apply the pre-filter.
///
/// Both writers of `display_name` go through this — [`submit`] and [`super::create`] — so there is
/// no route that reaches the queue without the filter, and no second place where the stored form
/// of a name is decided.
pub fn accept(raw: &str) -> Result<String, NameError> {
    let name = name_filter::normalise(raw);
    name_filter::check(&name).map_err(NameError::Refused)?;
    Ok(name)
}

/// Records a chosen name and puts it in the review queue.
///
/// On a rename the previously **approved** name stays public until the new one clears, so a rename
/// is not punished with anonymity: this writes `display_name` and never touches `public_name`.
pub fn submit(
    db: &Db,
    account_id: &str,
    name: &str,
    now: &str,
    cooldown_hours: i64,
) -> Result<NameState, NameError> {
    let name = accept(name)?;

    // The refusals are decided inside the transaction: the cooldown is read-then-write, and two
    // requests racing it would each see an expired clock and both spend the turn.
    db.write(|tx| {
        let (state, changed_at): (String, Option<String>) = tx.query_row(
            "SELECT name_state, name_changed_at FROM account WHERE id = ?1",
            params![account_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        // On the state, not on `display_name` being null: a refusal discards the name as well
        // (SC-018), so nullness stopped distinguishing "refused, pick another" from "erased,
        // never again" the moment reject started clearing it.
        if state == "erased" {
            return Ok(Err(NameError::Erased));
        }
        if let Some(retry_after_seconds) =
            cooldown_remaining(changed_at.as_deref(), now, cooldown_hours)
        {
            return Ok(Err(NameError::TooSoon {
                retry_after_seconds,
            }));
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

/// Publishes the name **the reviewer read**, not whatever the account holds now.
///
/// Keyed on the name rather than the account, because `approve(account_id)` alone is a
/// time-of-check-to-time-of-use hole and it is the exact hole pre-approval exists to close: the
/// holder resubmits between the reviewer reading the queue and clicking approve, and the UPDATE
/// publishes a string no human ever saw. Echoing the reviewed name back makes a swap a no-op —
/// [`Approval::Stale`] — instead of a publication.
///
/// Reversible, which is what allows it behind a public admin API (D25): approving a name puts it
/// on the board and nothing else, and [`reject`] takes it off again.
pub fn approve(db: &Db, account_id: &str, reviewed: &str) -> Result<Approval, DbError> {
    db.write(|tx| {
        // `display_name IS NOT NULL` keeps an erased account out. There is nothing to publish, and
        // publishing null would quietly re-mask an account that asked to be forgotten rather than
        // leaving it as it is.
        let n = tx.execute(
            "UPDATE account
                SET public_name = display_name, name_state = 'approved', name_reason = NULL
              WHERE id = ?1 AND display_name IS NOT NULL AND display_name = ?2",
            params![account_id, reviewed],
        )?;
        Ok(if n == 1 {
            Approval::Applied
        } else {
            Approval::Stale
        })
    })
}

/// Whether a review decision still applied to the name it was made about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    Applied,
    /// The name changed under the reviewer. Nothing was published; the queue has to be re-read.
    Stale,
}

/// Refuses the name the reviewer read, and **discards it**.
///
/// Keyed on the name for the same reason [`approve`] is: a decision made about one string must not
/// land on another.
///
/// The name is discarded rather than kept for the holder to look at again (SC-018, D25 as amended).
/// Holding a name you refused is holding personal data for no purpose, and the holder does not need
/// the string echoed back — they need to know it was refused and why, which `name_reason` carries.
/// `name_changed_at` is cleared, so a refusal does **not** consume the rename limit: the user has
/// not had a turn yet.
///
/// This is still reversible in the sense D25 requires. A leaked admin key can clear a name off the
/// board; it cannot destroy anything, and the holder puts a name back by submitting one.
pub fn reject(
    db: &Db,
    account_id: &str,
    reviewed: &str,
    reason: &str,
) -> Result<Approval, DbError> {
    db.write(|tx| {
        let n = tx.execute(
            "UPDATE account
                SET name_state = 'rejected', name_reason = ?3, public_name = NULL,
                    display_name = NULL, name_changed_at = NULL
              WHERE id = ?1 AND display_name IS NOT NULL AND display_name = ?2",
            params![account_id, reviewed, reason],
        )?;
        Ok(if n == 1 {
            Approval::Applied
        } else {
            Approval::Stale
        })
    })
}

/// Erases the name at the holder's own request (FR-035).
///
/// What is deleted is the name, not the history. The log is an append-only hash chain and no row of
/// it is ever rewritten; the account's trials stay in it under the opaque identifier and every proof
/// over them still verifies (FR-036). That is what FR-026 bought by keeping names out of the chain
/// in the first place, and it is the promise the data protection notice makes: erasure that costs
/// the record nothing, because the record never held the name.
///
/// Both name columns go, so the account is masked on every public surface from the next read on
/// (FR-047) while its rows and its figures stay exactly where they were.
///
/// **A name still in the queue disappears from it**, because `pending` selects on `display_name`
/// being present. A reviewer must not be handed a name whose owner has already asked for it to be
/// gone, and a review decision already in flight lands as [`Approval::Stale`] rather than
/// republishing it — both [`approve`] and [`reject`] match on `display_name`, which is now null. So
/// erasure wins any race with the review, which is the only direction that is safe to lose in.
///
/// Permanent, and marked by `name_state = 'erased'` rather than by `display_name` being null: a
/// refusal discards the name too (SC-018), so nullness distinguishes nothing, and the state is what
/// stops [`submit`] from ever setting one again.
///
/// Idempotent, and no error for an account that has no name to lose. `DELETE` asks for a state
/// rather than for an event, and a holder who clicks twice on a slow connection has not made a
/// mistake worth a message.
///
/// The account itself survives: the token still authenticates it and it still plays. Erasing a name
/// is not closing an account, and conflating the two would take a record out of the log that FR-036
/// says stays.
pub fn erase(db: &Db, account_id: &str, now: &str) -> Result<(), DbError> {
    db.write(|tx| {
        tx.execute(
            "UPDATE account
                SET display_name = NULL, public_name = NULL, name_state = 'erased',
                    name_reason = ?2, name_changed_at = ?3
              WHERE id = ?1",
            params![account_id, ERASED, now],
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

/// The accounts whose **published** name is exactly this string, as `(id, display_name,
/// public_name)`.
///
/// This exists so that a name already on the board can be taken off it through the API.
/// [`reject`] could always do that — it matches on the name, not on the state — but a reviewer had
/// no way to find the account behind a published name, and a takedown that needs an SSH session is
/// a takedown only the operator can do. That is the same argument D25 makes for the queue being
/// public, applied to the one decision that follows a mistake.
///
/// **Keyed on the exact name, never listed.** A `status=approved` that answered without a name
/// would turn a moderation surface into a bulk export of every name in the system, which is a
/// different thing to leak; the caller has to already know the string, and the string is on a
/// public leaderboard. Several accounts may hold the same name (FR-049), so this returns all of
/// them rather than assuming one.
///
/// Both names come back because they part company: after a holder submits a rename, `public_name`
/// is still what the board shows while `display_name` is what is waiting for review. [`reject`]
/// matches on `display_name`, so that is the string a caller has to send back, and returning only
/// the one that was searched for would hand out an argument that produces [`Approval::Stale`].
/// `public_name` being non-null implies `display_name` is too — every path that clears one clears
/// the other — but the query says so anyway rather than resting on it.
pub fn published_as(
    db: &Db,
    published: &str,
    limit: u32,
) -> Result<Vec<(String, String, String)>, DbError> {
    let r = db.reader()?;
    let mut stmt = r.prepare(
        "SELECT id, display_name, public_name FROM account
          WHERE public_name = ?1 AND display_name IS NOT NULL
          ORDER BY created_at, id
          LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![published, limit], |x| {
        Ok((x.get(0)?, x.get(1)?, x.get(2)?))
    })?;
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
    Ok(HolderView {
        name,
        state: state_from_column(&state),
        reason,
    })
}

/// The column is constrained to these three values, so anything else is a row this binary did not
/// write. Read as unpublished rather than refused: a name shown to nobody is the safe reading of a
/// state nobody understands.
fn state_from_column(raw: &str) -> NameState {
    match raw {
        "approved" => NameState::Approved,
        "rejected" => NameState::Rejected,
        "erased" => NameState::Erased,
        _ => NameState::Pending,
    }
}

/// Seconds left on the cooldown, or `None` if a name may be submitted now.
///
/// An unparsable stored timestamp counts as expired. This process writes the column with
/// `now_rfc3339`, so a value that will not parse means the row was edited by hand, and a hand edit
/// must not lock somebody out of their own name for ever.
fn cooldown_remaining(changed_at: Option<&str>, now: &str, hours: i64) -> Option<i64> {
    let last = OffsetDateTime::parse(changed_at?, &Rfc3339).ok()?;
    let now = OffsetDateTime::parse(now, &Rfc3339).ok()?;
    let allowed = last + Duration::hours(hours);
    (allowed > now).then(|| (allowed - now).whole_seconds().max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account;
    use crate::config::Config;

    const CREATED: &str = "2026-07-25T10:00:00Z";
    const NEXT_DAY: &str = "2026-07-26T10:00:01Z";
    const DAY_AFTER: &str = "2026-07-27T10:00:02Z";

    fn account_with_name(db: &Db, name: &str) -> String {
        account::create(db, name, CREATED)
            .expect("the name passes the filter")
            .id
    }

    /// [`submit`] at the cooldown the service ships with, read from [`Config`] rather than
    /// restated, so lowering the default is a change these tests run against.
    fn rename(db: &Db, id: &str, name: &str, now: &str) -> Result<NameState, NameError> {
        submit(db, id, name, now, Config::default().rename_cooldown_hours)
    }

    /// Reads `public_name`, the only column a public surface is allowed to look at.
    fn on_the_board(db: &Db, account_id: &str) -> String {
        let r = db.reader().unwrap();
        let public: Option<String> = r
            .query_row(
                "SELECT public_name FROM account WHERE id = ?1",
                params![account_id],
                |x| x.get(0),
            )
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
        approve(&db, &id, "otherfren").unwrap();
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

        reject(&db, &id, "otherfren", "hate").unwrap();
        let view = holder(&db, &id).unwrap();
        assert_eq!(
            view.name, None,
            "SC-018: a refused name is discarded, not kept for its owner to look at"
        );
        assert_eq!(view.state, NameState::Rejected);
        assert_eq!(
            view.reason.as_deref(),
            Some("hate"),
            "the reason is what the holder needs in order to pick a better one"
        );
    }

    /// FR-048: renaming is not punished with anonymity. The board keeps the old name for as long
    /// as the new one is in the queue.
    #[test]
    fn the_last_approved_name_stays_up_while_a_rename_is_reviewed() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");
        approve(&db, &id, "otherfren").unwrap();

        rename(&db, &id, "ganzfeld_enjoyer", NEXT_DAY).unwrap();
        assert_eq!(on_the_board(&db, &id), "otherfren");
        assert_eq!(
            holder(&db, &id).unwrap().name.as_deref(),
            Some("ganzfeld_enjoyer")
        );

        approve(&db, &id, "ganzfeld_enjoyer").unwrap();
        assert_eq!(on_the_board(&db, &id), "ganzfeld_enjoyer");
    }

    #[test]
    fn a_rename_inside_the_cooldown_is_refused() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        let too_soon = rename(&db, &id, "Monroe Institut", CREATED);
        assert!(matches!(too_soon, Err(NameError::TooSoon { .. })));
        assert!(rename(&db, &id, "Monroe Institut", NEXT_DAY).is_ok());
    }

    /// D25 in one test: a refusal costs the user nothing but the name, or the rate limit would
    /// punish them for the reviewer's decision.
    #[test]
    fn a_rejection_does_not_consume_the_rename_limit() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        reject(&db, &id, "otherfren", "hate").unwrap();
        // The very instant that was refused on the cooldown a moment ago.
        assert!(rename(&db, &id, "Monroe Institut", CREATED).is_ok());
    }

    /// Reversible in both directions, which is what bounds the blast radius of the public admin
    /// API (D25).
    #[test]
    fn a_name_can_be_pulled_off_the_board_and_put_back() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        approve(&db, &id, "otherfren").unwrap();
        reject(&db, &id, "otherfren", "hate").unwrap();
        assert_eq!(on_the_board(&db, &id), MASK);

        // Nothing was destroyed: the holder puts a name back by submitting one, which is the sense
        // in which D25 calls the admin API reversible. The refused string itself is gone (SC-018).
        rename(&db, &id, "Monroe Institut", CREATED).unwrap();
        approve(&db, &id, "Monroe Institut").unwrap();
        assert_eq!(on_the_board(&db, &id), "Monroe Institut");
    }

    /// The hole an adversarial review found: `approve(account_id)` published whatever the account
    /// held at UPDATE time, so a holder who resubmitted between the reviewer reading the queue and
    /// clicking approve got a string no human ever saw onto the board.
    #[test]
    fn approving_a_name_that_changed_underneath_publishes_nothing() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        // What the reviewer read.
        let queue = pending(&db, 10).unwrap();
        assert_eq!(queue[0].1, "otherfren");

        // What the holder swapped it for while the queue sat there.
        rename(&db, &id, "Monroe Institut", NEXT_DAY).unwrap();

        assert_eq!(
            approve(&db, &id, "otherfren").unwrap(),
            Approval::Stale,
            "the decision was about a name that is no longer there"
        );
        assert_eq!(on_the_board(&db, &id), MASK);
    }

    /// The tighter version of the same hole: rejection clears the rename cooldown, so the swap
    /// needs no waiting at all.
    #[test]
    fn rejecting_a_name_that_changed_underneath_decides_nothing() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");
        rename(&db, &id, "Monroe Institut", NEXT_DAY).unwrap();

        assert_eq!(
            reject(&db, &id, "otherfren", "hate").unwrap(),
            Approval::Stale
        );
        let view = holder(&db, &id).unwrap();
        assert_eq!(view.state, NameState::Pending, "still awaiting review");
        assert_eq!(view.name.as_deref(), Some("Monroe Institut"));
    }

    #[test]
    fn the_queue_is_pending_names_in_submission_order() {
        let db = Db::open_in_memory().unwrap();
        let first = account_with_name(&db, "otherfren");
        let second = account_with_name(&db, "Monroe Institut");
        approve(&db, &first, "otherfren").unwrap();

        assert_eq!(
            pending(&db, 10).unwrap(),
            vec![(second.clone(), "Monroe Institut".to_string())],
            "an approved name has left the queue"
        );

        // The older account renames, and joins the queue behind the name already waiting.
        rename(&db, &first, "ganzfeld_enjoyer", NEXT_DAY).unwrap();
        let queue: Vec<String> = pending(&db, 10)
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(queue, vec![second, first]);
    }

    /// FR-035: erasure is permanent, and the account keeps playing under its opaque identifier.
    #[test]
    fn an_erased_name_can_never_be_set_again() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");
        approve(&db, &id, "otherfren").unwrap();

        erase(&db, &id, NEXT_DAY).unwrap();
        assert_eq!(on_the_board(&db, &id), MASK);

        let view = holder(&db, &id).unwrap();
        assert_eq!(view.name, None);
        assert_eq!(view.reason.as_deref(), Some(ERASED));

        // Not merely rate-limited: a day later, and after a rejection, it is still refused.
        assert!(matches!(
            rename(&db, &id, "Monroe Institut", DAY_AFTER),
            Err(NameError::Erased)
        ));
        assert_eq!(
            reject(&db, &id, "otherfren", "hate").unwrap(),
            Approval::Stale,
            "an erased account has no name to refuse"
        );
        assert!(matches!(
            rename(&db, &id, "Monroe Institut", DAY_AFTER),
            Err(NameError::Erased)
        ));
        assert!(pending(&db, 10).unwrap().is_empty());
    }

    /// Erasure has to beat the review, or a name its owner has withdrawn is read out by a human
    /// and can still be published by a decision that was made a moment too late.
    #[test]
    fn erasing_a_name_takes_it_out_of_the_review_queue() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        // What the reviewer is holding while the holder asks for the name to be gone.
        assert_eq!(pending(&db, 10).unwrap().len(), 1);
        erase(&db, &id, NEXT_DAY).unwrap();

        assert!(pending(&db, 10).unwrap().is_empty());
        assert_eq!(
            approve(&db, &id, "otherfren").unwrap(),
            Approval::Stale,
            "a decision made before the erasure must not publish the name afterwards"
        );
        assert_eq!(on_the_board(&db, &id), MASK);
    }

    /// The contract calls `DELETE /api/account/name` idempotent, and this is where that has to be
    /// true: a second click on a slow connection is not a mistake to report.
    #[test]
    fn erasing_twice_changes_nothing_the_second_time() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        erase(&db, &id, NEXT_DAY).unwrap();
        let once = holder(&db, &id).unwrap();
        erase(&db, &id, DAY_AFTER).unwrap();
        let twice = holder(&db, &id).unwrap();

        assert_eq!(twice.name, None);
        assert_eq!(twice.state, once.state);
        assert_eq!(twice.reason, once.reason);
    }

    /// A name the pre-filter refuses must not reach the queue through `submit`, or the filter is
    /// only a client-side courtesy after all.
    #[test]
    fn the_pre_filter_cannot_be_skipped_by_submitting_directly() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        let refused = rename(&db, &id, "h1tl3r", NEXT_DAY);
        assert!(matches!(refused, Err(NameError::Refused(Refusal::Hate))));
        assert_eq!(holder(&db, &id).unwrap().name.as_deref(), Some("otherfren"));
    }

    #[test]
    fn a_submitted_name_is_stored_normalised() {
        let db = Db::open_in_memory().unwrap();
        let id = account_with_name(&db, "otherfren");

        rename(&db, &id, "  Monroe   Institut ", NEXT_DAY).unwrap();
        assert_eq!(
            holder(&db, &id).unwrap().name.as_deref(),
            Some("Monroe Institut")
        );
    }

    #[test]
    fn the_cooldown_counts_down_and_expires() {
        assert_eq!(cooldown_remaining(None, CREATED, 24), None);
        assert_eq!(
            cooldown_remaining(Some(CREATED), CREATED, 24),
            Some(24 * 3600)
        );
        assert_eq!(
            cooldown_remaining(Some(CREATED), "2026-07-25T22:00:00Z", 24),
            Some(12 * 3600)
        );
        assert_eq!(cooldown_remaining(Some(CREATED), NEXT_DAY, 24), None);
        assert_eq!(
            cooldown_remaining(Some("not a timestamp"), CREATED, 24),
            None
        );
    }
}
