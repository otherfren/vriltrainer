//! The public admin API of D25: list pending names, approve, reject.
//!
//! **Reversible operations only.** Every destructive operation — deleting an account, touching the
//! log, changing pool versions — stays a CLI subcommand behind SSH, so a leaked admin key costs an
//! embarrassing name on the board for an hour and not the audit log. That bound is what lets this
//! API be public and have one privilege level instead of roles and scopes.
//!
//! Public rather than loopback because the reviewers are not only the operator: a queue that can
//! only be worked from an SSH session is a queue that only one person can work, and D25 already
//! admits the operator is the bottleneck. Nothing here may become the exception — if a route in
//! this file ever destroys something, the argument for the whole surface being public collapses
//! with it.
//!
//! A decision is made about a **name**, not about an account. Both handlers take the string the
//! reviewer read and pass it to [`name::approve`] / [`name::reject`], which apply only if it is
//! still there. Dropping that parameter reintroduces the hole an adversarial review already
//! found: the holder resubmits between the queue being read and the button being pressed, and a
//! human publishes a string nobody ever saw.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{LazyLock, Mutex};

use axum::Router;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};

use crate::account::admin_key;
use crate::account::name::{self, Approval};
use crate::db::now_rfc3339;
use crate::http::client_addr::ClientAddr;
use crate::http::{ApiError, AppState};
use crate::trial::timing::now_unix;

/// Names returned by one read of the queue.
///
/// No cursor, deliberately: the queue is worked by deciding, and every decision removes a row from
/// it. A name still pending after a hundred approvals is on the next page by construction, and a
/// paging scheme would be state to get right for a list that shrinks as it is read.
const PAGE: u32 = 100;

/// The only queue this API serves.
const PENDING: &str = "pending";

/// Reason codes a reviewer may give. The first seven are the pre-filter's own vocabulary — see the
/// drift test below — so the client renders one table of refusal strings rather than two.
///
/// `refused` exists because a human rejects names the filter has no word for. The list is closed
/// on purpose: a free-text reason would be untranslated product copy stored in the database and
/// shown verbatim to the holder, which is both a German string in the wrong layer (CLAUDE.md) and
/// a channel a reviewer could use to write whatever they liked into somebody's account.
const REASONS: [&str; 8] = [
    "too_short",
    "too_long",
    "shapeless",
    "reserved",
    "hate",
    "vulgar",
    "address",
    "refused",
];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/names", get(queue))
        .route("/admin/names/{id}/approve", post(approve))
        .route("/admin/names/{id}/reject", post(reject))
}

/// The identifier of the key a request presented.
///
/// Not the key, and not a "role": there is one privilege level (D25). What this carries is enough
/// to attribute a decision to the credential that made it, which is the only thing a second
/// reviewer changes about the model.
pub struct Reviewer(pub String);

impl FromRequestParts<AppState> for Reviewer {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let ClientAddr(addr) = match ClientAddr::from_request_parts(parts, state).await {
            Ok(addr) => addr,
            // `ClientAddr` cannot fail; it warns and falls back. See `http::client_addr`.
            Err(never) => match never {},
        };
        // Before the key is looked at, so a caller cannot make this process do database work by
        // presenting one wrong key after another.
        if !attempts().admits(addr, now_unix()) {
            return Err(ApiError::RateLimited);
        }

        let presented = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(bearer)
            .ok_or(ApiError::Unauthorized)?;

        match admin_key::authenticate(&state.db, presented, &now_rfc3339())? {
            Some(id) => Ok(Reviewer(id)),
            None => Err(ApiError::Unauthorized),
        }
    }
}

/// The credential out of an `Authorization` header, scheme matched case-insensitively per RFC
/// 7235.
///
/// A copy of the player-side reader in [`super::account`] rather than a shared helper. The two
/// credentials are checked against different tables and mean different things, and a function used
/// by both is the place where one day one of them gets checked as the other.
fn bearer(header: &str) -> Option<&str> {
    let (scheme, credential) = header.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| credential.trim())
        .filter(|c| !c.is_empty())
}

#[derive(Deserialize)]
struct QueueQuery {
    status: Option<String>,
}

#[derive(Serialize)]
struct QueueResponse {
    status: &'static str,
    names: Vec<PendingName>,
}

#[derive(Serialize)]
struct PendingName {
    /// The opaque account identifier, which is what the decision routes take back. The name is
    /// never a key here: names are not unique (FR-049).
    account_id: String,
    name: String,
}

/// The review queue, oldest submission first.
///
/// `status` is accepted and constrained to `pending` rather than ignored, so the query string in
/// `contracts` is honoured and a reviewer's tool asking for something else is told no. Widening it
/// to `approved` would turn a name-approval surface into a bulk export of every name in the
/// system, which is a different thing to leak.
async fn queue(
    State(state): State<AppState>,
    Reviewer(_): Reviewer,
    Query(query): Query<QueueQuery>,
) -> Result<Response, ApiError> {
    if query.status.as_deref().unwrap_or(PENDING) != PENDING {
        return Err(ApiError::BadRequest("status must be pending"));
    }

    let names = name::pending(&state.db, PAGE)?
        .into_iter()
        .map(|(account_id, name)| PendingName { account_id, name })
        .collect();

    Ok(Json(QueueResponse {
        status: PENDING,
        names,
    })
    .into_response())
}

#[derive(Deserialize)]
struct ApprovalRequest {
    /// The name the reviewer read. Required, and passed straight through: this parameter is the
    /// whole defence against publishing a string no human saw.
    name: String,
}

/// Publishes the name the reviewer read (D25, FR-047).
async fn approve(
    State(state): State<AppState>,
    Reviewer(key): Reviewer,
    Path(account_id): Path<String>,
    Json(request): Json<ApprovalRequest>,
) -> Result<Response, ApiError> {
    let outcome = name::approve(&state.db, &account_id, &request.name)?;
    decided(&key, &account_id, "approve", None, outcome)
}

#[derive(Deserialize)]
struct RejectionRequest {
    name: String,
    reason: String,
}

/// Refuses the name the reviewer read, and discards it (SC-018).
///
/// Reversible in the sense D25 means: the name comes off the board and the holder puts one back by
/// submitting another — immediately, because a rejection does not consume the rename cooldown.
async fn reject(
    State(state): State<AppState>,
    Reviewer(key): Reviewer,
    Path(account_id): Path<String>,
    Json(request): Json<RejectionRequest>,
) -> Result<Response, ApiError> {
    let reason = REASONS
        .iter()
        .find(|r| **r == request.reason)
        .ok_or(ApiError::BadRequest("unknown reason"))?;

    let outcome = name::reject(&state.db, &account_id, &request.name, reason)?;
    decided(&key, &account_id, "reject", Some(reason), outcome)
}

/// The answer to a decision, and the audit line that goes with it.
///
/// A stale decision is a `409` and not a quiet `200`. The reviewer has to re-read the queue: told
/// "done", they would believe they had cleared a name that is still sitting there, and the one
/// state D25 cannot tolerate is a human believing a name was looked at when it was not.
///
/// The line names the key, the account and the outcome — and **never the name**. A rejected name
/// is exactly the string D25 says to discard; writing it to journald keeps a copy of the slur in
/// the one place nobody thinks to clear out.
fn decided(
    key: &str,
    account_id: &str,
    action: &'static str,
    reason: Option<&str>,
    outcome: Approval,
) -> Result<Response, ApiError> {
    match outcome {
        Approval::Applied => {
            tracing::info!(key, account = account_id, action, reason, "name reviewed");
            Ok(Json(serde_json::json!({ "outcome": "applied" })).into_response())
        }
        Approval::Stale => {
            tracing::info!(
                key,
                account = account_id,
                action,
                "a review decision named a name that is no longer there"
            );
            let body = Json(serde_json::json!({ "error": "stale" }));
            Ok((StatusCode::CONFLICT, body).into_response())
        }
    }
}

/// Admin requests one client address may make inside [`WINDOW_SECONDS`].
///
/// Generous, because the legitimate heavy user is a reviewer working through a queue and locking
/// one out is a worse day than the flood this prevents. It counts **every** request, refused ones
/// included — a limiter that only counted failures would be spent by the caller it is meant to
/// stop and never by anyone else, which is backwards.
///
/// It is not what makes the key hard to guess; 256 bits from a CSPRNG is. What it bounds is the
/// database work an unauthenticated caller can make this process do on the one route that reads a
/// credential table. Rotating addresses defeats it, and at that point the request flood is nginx's
/// problem rather than this file's.
const PER_WINDOW: u32 = 60;
const WINDOW_SECONDS: i64 = 60;

/// Addresses remembered at once. Small, because the population of this surface is a handful of
/// reviewers; a flood from more distinct addresses than this is already the case below.
const MAX_TRACKED: usize = 4096;

/// In memory and never a table, for the same reason [`crate::http::limits::CreationLimit`] is:
/// D28 refuses a per-visitor row, and a list of addresses that touched the admin API is exactly
/// one. A separate counter from that one because it counts a different thing over a different
/// window — sharing them would make the account-creation allowance depend on moderation traffic.
struct Attempts {
    seen: Mutex<HashMap<IpAddr, Vec<i64>>>,
}

impl Attempts {
    /// Counts this request and says whether it is inside the allowance.
    fn admits(&self, addr: IpAddr, now: i64) -> bool {
        let mut seen = lock(&self.seen);
        if seen.len() >= MAX_TRACKED {
            seen.retain(|_, at| at.iter().any(|t| *t > now - WINDOW_SECONDS));
            if seen.len() >= MAX_TRACKED && !seen.contains_key(&addr) {
                // Nothing left to forget, so an unknown address is admitted. Fails open like the
                // creation limit: the key is what protects this surface, and a limiter that shuts
                // the reviewers out under load has turned a nuisance into the outage.
                return true;
            }
        }
        let at = seen.entry(addr).or_default();
        at.retain(|t| *t > now - WINDOW_SECONDS);
        at.push(now);
        at.len() <= PER_WINDOW as usize
    }
}

/// The process's counter. A global for the same reason the creation limit is one: it is not
/// per-request state and the application state is rebuilt per test.
fn attempts() -> &'static Attempts {
    static ATTEMPTS: LazyLock<Attempts> = LazyLock::new(|| Attempts {
        seen: Mutex::new(HashMap::new()),
    });
    &ATTEMPTS
}

/// A poisoned lock means a previous caller panicked while counting; see `db::lock`.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::body::Body;
    use axum::extract::connect_info::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;
    use crate::account;
    use crate::account::name_filter::Refusal;
    use crate::db::Db;
    use crate::http::{router, test_support};

    /// A request as nginx would deliver it. The address is forged per test on purpose: the
    /// attempt counter is one counter for the whole process, so two tests sharing an address
    /// would spend each other's allowance.
    fn request(method: &str, uri: &str, from: &str) -> axum::http::request::Builder {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-forwarded-for", from);
        // The peer is a trusted proxy by default, so the forwarded address is believed (R8).
        let peer: SocketAddr = "127.0.0.1:44321".parse().unwrap();
        builder
            .extensions_mut()
            .expect("the builder has no error yet")
            .insert(ConnectInfo(peer));
        builder
    }

    fn signed(
        method: &str,
        uri: &str,
        from: &str,
        key: &str,
        body: serde_json::Value,
    ) -> Request<Body> {
        request(method, uri, from)
            .header("authorization", format!("Bearer {key}"))
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn call(state: &AppState, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let response = router(state.clone()).oneshot(req).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    /// A state with one admin key already issued, and that key.
    fn reviewed() -> (AppState, String) {
        let state = test_support::state();
        let key = admin_key::rotate(&state.db, "test", &now_rfc3339())
            .unwrap()
            .key;
        (state, key)
    }

    fn holder(db: &Db, name: &str) -> String {
        account::create(db, name, &now_rfc3339())
            .expect("the fixture name passes the filter")
            .id
    }

    fn on_the_board(db: &Db, account_id: &str) -> String {
        let r = db.reader().unwrap();
        let public: Option<String> = r
            .query_row(
                "SELECT public_name FROM account WHERE id = ?1",
                rusqlite::params![account_id],
                |x| x.get(0),
            )
            .unwrap();
        name::public_display(public.as_deref())
    }

    #[tokio::test]
    async fn the_queue_is_the_pending_names_and_needs_a_key() {
        let (state, key) = reviewed();
        let id = holder(&state.db, "otherfren");

        let (status, _) = call(
            &state,
            request("GET", "/admin/names", "203.0.113.20")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "the queue is not public");

        let (status, _) = call(
            &state,
            signed(
                "GET",
                "/admin/names",
                "203.0.113.21",
                "not the key",
                serde_json::json!({}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (status, body) = call(
            &state,
            signed(
                "GET",
                "/admin/names?status=pending",
                "203.0.113.22",
                &key,
                serde_json::json!({}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["names"][0]["account_id"], id);
        assert_eq!(body["names"][0]["name"], "otherfren");
    }

    #[tokio::test]
    async fn a_status_other_than_pending_is_refused() {
        let (state, key) = reviewed();
        let (status, body) = call(
            &state,
            signed(
                "GET",
                "/admin/names?status=approved",
                "203.0.113.23",
                &key,
                serde_json::json!({}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "status must be pending");
    }

    /// D25 end to end: nothing is public until a human says so, and then it is.
    #[tokio::test]
    async fn approving_the_reviewed_name_publishes_it() {
        let (state, key) = reviewed();
        let id = holder(&state.db, "otherfren");
        assert_eq!(on_the_board(&state.db, &id), name::MASK);

        let (status, body) = call(
            &state,
            signed(
                "POST",
                &format!("/admin/names/{id}/approve"),
                "203.0.113.24",
                &key,
                serde_json::json!({ "name": "otherfren" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["outcome"], "applied");
        assert_eq!(on_the_board(&state.db, &id), "otherfren");
    }

    /// The hole an adversarial review found, at the HTTP layer this time: the reviewed name has to
    /// reach [`name::approve`], or a holder who resubmits in the meantime publishes a string
    /// nobody read.
    #[tokio::test]
    async fn approving_a_name_that_changed_underneath_is_a_conflict_and_publishes_nothing() {
        let (state, key) = reviewed();
        let id = holder(&state.db, "otherfren");

        // What the holder swapped it for while the queue sat on the reviewer's screen.
        name::submit(
            &state.db,
            &id,
            "Monroe Institut",
            "2099-01-01T00:00:00Z",
            state.config.rename_cooldown_hours,
        )
        .unwrap();

        let (status, body) = call(
            &state,
            signed(
                "POST",
                &format!("/admin/names/{id}/approve"),
                "203.0.113.25",
                &key,
                serde_json::json!({ "name": "otherfren" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "stale");
        assert_eq!(
            on_the_board(&state.db, &id),
            name::MASK,
            "a decision about a name that is gone must publish nothing"
        );
    }

    #[tokio::test]
    async fn rejecting_takes_the_name_off_the_board_and_the_holder_may_choose_again() {
        let (state, key) = reviewed();
        let id = holder(&state.db, "otherfren");

        let reject = |from: &'static str, reason: &'static str| {
            signed(
                "POST",
                &format!("/admin/names/{id}/reject"),
                from,
                &key,
                serde_json::json!({ "name": "otherfren", "reason": reason }),
            )
        };

        let (status, body) = call(&state, reject("203.0.113.26", "not a reason")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "unknown reason");

        let (status, body) = call(&state, reject("203.0.113.27", "hate")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["outcome"], "applied");

        let view = name::holder(&state.db, &id).unwrap();
        assert_eq!(view.reason.as_deref(), Some("hate"));
        assert_eq!(view.name, None, "SC-018: a refused name is discarded");
        assert_eq!(on_the_board(&state.db, &id), name::MASK);

        // Reversible in the sense D25 means: the holder puts a name back at once, because a
        // rejection does not consume the rename cooldown.
        name::submit(
            &state.db,
            &id,
            "ganzfeld_enjoyer",
            &now_rfc3339(),
            state.config.rename_cooldown_hours,
        )
        .unwrap();
    }

    /// The bound on a leaked key, asserted rather than assumed. Nothing on this surface removes an
    /// account, edits the log or touches a pool version — so those addresses are not routes.
    #[tokio::test]
    async fn nothing_destructive_is_mounted() {
        let (state, key) = reviewed();
        let id = holder(&state.db, "otherfren");

        for (method, uri) in [
            ("DELETE", format!("/admin/names/{id}")),
            ("DELETE", format!("/admin/accounts/{id}")),
            ("POST", format!("/admin/accounts/{id}/delete")),
            ("DELETE", "/admin/log".to_string()),
            ("POST", "/admin/pool".to_string()),
            ("POST", "/admin/keys".to_string()),
        ] {
            let (status, _) = call(
                &state,
                signed(method, &uri, "203.0.113.28", &key, serde_json::json!({})),
            )
            .await;
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "{method} {uri} exists on a surface that must only ever be reversible"
            );
        }
    }

    /// The claim D25 makes about putting the hash in the database: the reviewers' key changes and
    /// the running process notices, with nothing restarted. Same router, same state, same process.
    #[tokio::test]
    async fn a_rotation_takes_effect_without_a_restart() {
        let (state, old) = reviewed();
        let queue = |from: &'static str, key: String| {
            signed("GET", "/admin/names", from, &key, serde_json::json!({}))
        };

        assert_eq!(
            call(&state, queue("203.0.113.29", old.clone())).await.0,
            StatusCode::OK
        );

        let new = admin_key::rotate(&state.db, "second reviewer", &now_rfc3339())
            .unwrap()
            .key;

        assert_eq!(
            call(&state, queue("203.0.113.30", old)).await.0,
            StatusCode::UNAUTHORIZED,
            "the retired key must stop working at once"
        );
        assert_eq!(
            call(&state, queue("203.0.113.31", new)).await.0,
            StatusCode::OK
        );
    }

    /// The limit counts every request, so a caller cannot spend its allowance on guesses and then
    /// get an unlimited run at the real thing.
    #[tokio::test]
    async fn the_admin_surface_is_rate_limited_per_address() {
        let (state, key) = reviewed();

        for _ in 0..PER_WINDOW {
            let (status, _) = call(
                &state,
                signed(
                    "GET",
                    "/admin/names",
                    "203.0.113.32",
                    "wrong",
                    serde_json::json!({}),
                ),
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
        }

        let (status, _) = call(
            &state,
            signed(
                "GET",
                "/admin/names",
                "203.0.113.32",
                &key,
                serde_json::json!({}),
            ),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::TOO_MANY_REQUESTS,
            "the allowance is spent by refused requests too"
        );

        // Another address is unaffected: the limit is per client, not global.
        let (status, _) = call(
            &state,
            signed(
                "GET",
                "/admin/names",
                "203.0.113.33",
                &key,
                serde_json::json!({}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    /// [`REASONS`] must stay a superset of the pre-filter's vocabulary, or a reviewer cannot give
    /// the reason the filter itself would have given. The `match` is exhaustive on purpose: a new
    /// [`Refusal`] variant fails to compile here rather than quietly becoming unsayable.
    #[test]
    fn the_reason_vocabulary_covers_the_filters_own() {
        fn code(refusal: Refusal) -> &'static str {
            match refusal {
                Refusal::TooShort => "too_short",
                Refusal::TooLong => "too_long",
                Refusal::Shapeless => "shapeless",
                Refusal::Reserved => "reserved",
                Refusal::Hate => "hate",
                Refusal::Vulgar => "vulgar",
                Refusal::Address => "address",
            }
        }

        for refusal in [
            Refusal::TooShort,
            Refusal::TooLong,
            Refusal::Shapeless,
            Refusal::Reserved,
            Refusal::Hate,
            Refusal::Vulgar,
            Refusal::Address,
        ] {
            let code = code(refusal);
            assert!(
                REASONS.contains(&code),
                "{code} is not a reason a reviewer can give"
            );
            // And it is the spelling the client already receives from `POST /api/account`.
            assert_eq!(serde_json::to_value(refusal).unwrap(), code);
        }
    }

    #[test]
    fn the_counter_admits_up_to_the_allowance_and_keeps_addresses_apart() {
        let attempts = Attempts {
            seen: Mutex::new(HashMap::new()),
        };
        let one: IpAddr = "203.0.113.7".parse().unwrap();
        let two: IpAddr = "198.51.100.9".parse().unwrap();
        const NOW: i64 = 1_800_000_000;

        for _ in 0..PER_WINDOW {
            assert!(attempts.admits(one, NOW));
        }
        assert!(!attempts.admits(one, NOW));
        assert!(attempts.admits(two, NOW));
        // The window slides, so an address is not shut out for ever by one busy minute.
        assert!(attempts.admits(one, NOW + WINDOW_SECONDS));
    }
}
