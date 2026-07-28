//! `POST /api/account`, `DELETE /api/account/name`, and the rename of FR-048.
//!
//! Also the home of [`Holder`], the extractor that turns a bearer token into an account. It lives
//! here rather than beside the router because "who is this" is an account question, and there is
//! exactly one answer to it: the token hash matches a row or it does not. There is no session, no
//! second factor and no recovery to fall back to (D9).

use axum::Router;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{post, put};
use serde::{Deserialize, Serialize};

use crate::account::{self, name::NameError, name_filter::Refusal};
use crate::db::now_rfc3339;
use crate::http::{ApiError, AppState};
use crate::metrics;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/account", post(create).get(whoami))
        .route("/api/account/name", put(rename).delete(erase_name))
}

/// The authenticated account's opaque identifier.
///
/// Deliberately not the whole account row: a handler holding this can write to the log, and the
/// log carries the identifier and never the name (FR-026). Anything that wants the name has to go
/// and ask for it, which makes that step visible in the code.
pub struct Holder(pub String);

impl FromRequestParts<AppState> for Holder {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let presented = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(bearer)
            .ok_or(ApiError::Unauthorized)?;

        // The token never reaches a log line, an error body or a metric. It *is* the account (D9),
        // and the whole reason it travels in a URL fragment is that nothing records it.
        match account::authenticate(&state.db, presented)? {
            Some(id) => Ok(Holder(id)),
            None => Err(ApiError::Unauthorized),
        }
    }
}

/// The credential out of an `Authorization` header. The scheme is case-insensitive per RFC 7235,
/// and a client that sends `bearer` is not wrong — refusing it would cost somebody an afternoon.
fn bearer(header: &str) -> Option<&str> {
    let (scheme, credential) = header.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| credential.trim())
        .filter(|c| !c.is_empty())
}

#[derive(Deserialize)]
struct CreateRequest {
    name: String,
}

#[derive(Serialize)]
struct CreateResponse {
    public_id: String,
    /// Returned **once**. Only its hash is stored and there is no recovery (FR-002, FR-005, D9).
    access_token: String,
    /// The name as stored — trimmed and collapsed — not necessarily what was typed. Echoing the
    /// submitted string back instead would leave the client displaying a name the server does not
    /// hold.
    name: String,
}

/// Creates an account from a self-chosen name.
///
/// The name lands `pending` and is masked on every public surface until a human approves it
/// (D25, FR-047). The response carries it back anyway, because the holder is not a stranger and
/// always sees their own name whatever state it is in.
async fn create(
    State(state): State<AppState>,
    Json(request): Json<CreateRequest>,
) -> Result<Response, ApiError> {
    let now = now_rfc3339();

    // The sweep of D32 hangs off this route because this route is what fills the table, and it runs
    // **before** the insert rather than after it. Interval-gated, so it is a no-op on all but one
    // request an hour; when it does run and the database refuses it, the visitor must not already be
    // holding the one copy of a token whose row may not have survived.
    account::reap::ensure_swept(&state.db, state.config.as_ref(), &now)?;

    let account = account::create(&state.db, &request.name, &now).map_err(refusal)?;
    // Counted after the insert, so a refused name is not an account in the figures (FR-052).
    state.metrics.count(metrics::name::ACCOUNT_CREATED, &now);
    state.metrics.count(metrics::name::NAME_SUBMITTED, &now);

    Ok((
        StatusCode::CREATED,
        Json(CreateResponse {
            public_id: account.public_id,
            access_token: account.access_token,
            name: account.name,
        }),
    )
        .into_response())
}

#[derive(Serialize)]
struct WhoamiResponse {
    public_id: String,
    /// Null after erasure, and after a refusal discarded what was submitted.
    name: Option<String>,
    name_state: String,
}

/// Who the bearer of this token is.
///
/// The access link is a capability and carries no identity (D9), so a browser that arrived through
/// one holds a token and knows nothing about whose account it opens. Everything else it could ask
/// for — `GET /api/stats/me` — answers with figures and no name, which is how the header ended up
/// showing a placeholder for the rest of the session.
async fn whoami(
    State(state): State<AppState>,
    Holder(account): Holder,
) -> Result<Response, ApiError> {
    // A token that authenticated but whose row is gone is not a 404 about a missing page — it is
    // this token no longer opening anything, which is what 401 says.
    let own = account::own(&state.db, &account)?.ok_or(ApiError::Unauthorized)?;

    Ok(Json(WhoamiResponse {
        public_id: own.public_id,
        name: own.name,
        name_state: own.name_state,
    })
    .into_response())
}

#[derive(Deserialize)]
struct RenameRequest {
    name: String,
}

/// Changes the holder's name, subject to the cooldown (FR-048).
///
/// `PUT` rather than `POST`: the request names the state the holder wants their name to be in, and
/// sending the same name twice inside the cooldown has to fail for the *same* reason a different
/// name does — the turn is spent — rather than because the second call was a different kind of
/// request.
///
/// The response is the shape `GET /api/account` returns, because after a rename the client needs
/// exactly what that endpoint carries: the stored form of the name and the state it is now in. It
/// is `pending`, always — every name goes through review (D25), including one that was approved
/// before.
///
/// **The previously approved name stays on the board** while this one is reviewed, which is
/// [`account::name::submit`]'s doing. A rename is not punished with anonymity.
///
/// The cooldown answers `429` and not `400`, with `Retry-After`: the name was not refused, the
/// request was early, and the one thing the client can do about it is wait a stated number of
/// seconds. A refusal by the pre-filter is a `400` with its code, and a **refusal does not consume
/// the cooldown** — [`account::name::reject`] clears the clock, so the holder has not had a turn.
async fn rename(
    State(state): State<AppState>,
    Holder(account): Holder,
    Json(request): Json<RenameRequest>,
) -> Result<Response, ApiError> {
    let now = now_rfc3339();
    let submitted = account::name::submit(
        &state.db,
        &account,
        &request.name,
        &now,
        state.config.rename_cooldown_hours,
    );

    if let Err(NameError::TooSoon {
        retry_after_seconds,
    }) = submitted
    {
        return Ok(too_soon(retry_after_seconds));
    }
    submitted.map_err(refusal)?;
    state.metrics.count(metrics::name::NAME_SUBMITTED, &now);

    // Read back rather than assembling from what was sent: the stored form is trimmed and
    // collapsed, and a client that displays the typed string displays a name the server does not
    // hold.
    let own = account::own(&state.db, &account)?.ok_or(ApiError::Unauthorized)?;
    Ok(Json(WhoamiResponse {
        public_id: own.public_id,
        name: own.name,
        name_state: own.name_state,
    })
    .into_response())
}

/// A rename inside the cooldown: `429`, the code, and how long the wait is.
///
/// `Retry-After` carries the seconds as well as the body, because it is the header a proxy or a
/// script already knows how to read and the body is for the interface copy.
fn too_soon(retry_after_seconds: i64) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::RETRY_AFTER, retry_after_seconds.to_string())],
        Json(serde_json::json!({
            "error": "too_soon",
            "retry_after_seconds": retry_after_seconds,
        })),
    )
        .into_response()
}

/// Removes the holder's name, for good (FR-035).
///
/// Self-service and authenticated by nothing but the access token, because the token is the only
/// proof of ownership that exists (D9) — an erasure that had to go through the operator would be a
/// support address on a site with no accounts to look anyone up by, and a promise in the data
/// protection notice that one person's inbox has to keep.
///
/// `204` and no body: there is nothing left to describe, and the client already knows what it asked
/// for. Idempotent by the contract, so a second call is another `204` rather than a `404` about a
/// name that is already gone.
///
/// **The log is not touched.** The account's trials stay in the record under the opaque identifier
/// and stay verifiable (FR-036) — see [`account::name::erase`], which is where that holds.
async fn erase_name(
    State(state): State<AppState>,
    Holder(account): Holder,
) -> Result<Response, ApiError> {
    account::name::erase(&state.db, &account, &now_rfc3339())?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// A refused name is a 400 carrying the machine-readable code, never a sentence: the sentence is
/// product copy, it differs per domain (D10), and it lives in the client's message catalogue.
fn refusal(e: NameError) -> ApiError {
    match e {
        NameError::Db(e) => ApiError::Db(e),
        NameError::Refused(why) => ApiError::BadRequest(match why {
            Refusal::TooShort => "too_short",
            Refusal::TooLong => "too_long",
            Refusal::Shapeless => "shapeless",
            Refusal::Reserved => "reserved",
            Refusal::Hate => "hate",
            Refusal::Vulgar => "vulgar",
            Refusal::Address => "address",
        }),
        // Unreachable from creation — there is no earlier name to be inside a cooldown for, and
        // nothing to have erased — and the rename answers the cooldown itself, with `429` and the
        // wait, before it ever gets here. Answered rather than unwrapped, because a panic in a
        // handler is a 500 with nothing attached to explain it.
        NameError::TooSoon { .. } => ApiError::BadRequest("too_soon"),
        // Reachable from the rename, and permanent: erasure is not a state a name comes back from
        // (FR-035), so the client is told that rather than being offered a field again.
        NameError::Erased => ApiError::BadRequest("erased"),
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::body::Body;
    use axum::extract::connect_info::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::account::name::{self, MASK};
    use crate::config::Config;
    use crate::http::routes::stats::test_support::{Fixture, json};
    use crate::http::{AppState, router, test_support};

    /// A request that looks like it came through nginx from `forwarded`.
    ///
    /// The address is forged per test on purpose: the creation limit is one counter for the whole
    /// process, so two tests sharing an address would spend each other's allowance.
    fn create_request(forwarded: &str, name: &str) -> Request<Body> {
        let mut request = Request::builder()
            .method("POST")
            .uri("/api/account")
            .header("content-type", "application/json")
            .header("x-forwarded-for", forwarded)
            .body(Body::from(format!("{{\"name\":\"{name}\"}}")))
            .unwrap();
        // The peer is a trusted proxy by default, so the forwarded address is believed (R8).
        let peer: SocketAddr = "127.0.0.1:44321".parse().unwrap();
        request.extensions_mut().insert(ConnectInfo(peer));
        request
    }

    async fn body(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn creating_an_account_returns_the_token_once() {
        let state = test_support::state();
        let response = router(state.clone())
            .oneshot(create_request("203.0.113.10", "otherfren"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let json = body(response).await;
        assert_eq!(json["name"], "otherfren");
        assert_eq!(json["public_id"].as_str().unwrap().len(), 6);
        let token = json["access_token"].as_str().unwrap().to_string();

        // The account is real and the token opens it — and this token is never issued again,
        // because no endpoint returns one.
        assert!(
            crate::account::authenticate(&state.db, &token)
                .unwrap()
                .is_some()
        );
    }

    /// D25: a new name is not public. The response carries it because the holder is not a
    /// stranger, but nothing has been approved.
    #[tokio::test]
    async fn a_new_name_is_stored_normalised_and_not_yet_public() {
        let state = test_support::state();
        let response = router(state.clone())
            .oneshot(create_request("203.0.113.11", "  Monroe   Institut "))
            .await
            .unwrap();
        assert_eq!(body(response).await["name"], "Monroe Institut");

        let reader = state.db.reader().unwrap();
        let public: Option<String> = reader
            .query_row("SELECT public_name FROM account", [], |r| r.get(0))
            .unwrap();
        assert_eq!(public, None, "nothing is public before a human approves it");
    }

    #[tokio::test]
    async fn a_refused_name_is_a_four_hundred_with_its_code() {
        let response = router(test_support::state())
            .oneshot(create_request("203.0.113.12", "h1tl3r"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body(response).await["error"], "hate");
    }

    /// The whole reason `GET /api/account` exists: an access link carries a capability and no
    /// identity, so a browser holding only a token must be able to ask whose account it opens.
    #[tokio::test]
    async fn the_holder_can_ask_who_they_are() {
        let state = test_support::state();
        let created = body(
            router(state.clone())
                .oneshot(create_request("203.0.113.20", "otherfren"))
                .await
                .unwrap(),
        )
        .await;
        let token = created["access_token"].as_str().unwrap().to_string();

        let response = router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/api/account")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = body(response).await;
        assert_eq!(json["public_id"], created["public_id"]);
        // D25: the holder sees their own name whatever state it is in, and a new name is pending.
        assert_eq!(json["name"], "otherfren");
        assert_eq!(json["name_state"], "pending");
    }

    #[tokio::test]
    async fn a_stranger_cannot_ask_who_somebody_is() {
        let state = test_support::state();
        let response = router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/account")
                    .header("authorization", "Bearer not-a-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// The per-address creation limit of D17 was removed by the operator. This is the test that
    /// keeps it removed: it fails the moment something starts counting accounts per address again.
    #[tokio::test]
    async fn one_address_may_create_as_many_accounts_as_it_likes() {
        let state = test_support::state();

        for _ in 0..12 {
            let response = router(state.clone())
                .oneshot(create_request("203.0.113.13", "otherfren"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
        }
    }

    fn authenticated(method: &str, uri: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    }

    /// An authenticated request carrying a name, which is every rename.
    fn rename_request(token: &str, name: &str) -> Request<Body> {
        Request::builder()
            .method("PUT")
            .uri("/api/account/name")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(format!("{{\"name\":\"{name}\"}}")))
            .unwrap()
    }

    /// An account with an approved name, which is the state a rename starts from.
    async fn approved(state: &AppState, forwarded: &str, name: &str) -> String {
        let created = json(call(state, create_request(forwarded, name)).await).await;
        let token = created["access_token"].as_str().unwrap().to_string();
        let id = crate::account::authenticate(&state.db, &token)
            .unwrap()
            .unwrap();
        name::approve(&state.db, &id, name).unwrap();
        token
    }

    async fn call(state: &AppState, request: Request<Body>) -> axum::response::Response {
        router(state.clone()).oneshot(request).await.unwrap()
    }

    async fn public(state: &AppState, uri: &str) -> serde_json::Value {
        let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
        json(call(state, request).await).await
    }

    /// FR-035, the shape the contract asks for: `204`, no body, and both name columns empty. The
    /// queue is checked here as well, because a name withdrawn from review is the case where an
    /// incomplete erasure would still put the name in front of a human.
    #[tokio::test]
    async fn erasing_a_name_answers_204_and_leaves_nothing_behind() {
        let state = test_support::state();
        let created = json(call(&state, create_request("203.0.113.30", "otherfren")).await).await;
        let token = created["access_token"].as_str().unwrap().to_string();
        name::approve(
            &state.db,
            &crate::account::authenticate(&state.db, &token)
                .unwrap()
                .unwrap(),
            "otherfren",
        )
        .unwrap();

        let response = call(&state, authenticated("DELETE", "/api/account/name", &token)).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap()
                .is_empty(),
            "there is nothing left to describe"
        );

        // Scoped: an in-memory reader borrows the writer, and anything that reads while it lives
        // waits for it — including `pending` below.
        let (display, public_name, state_column): (Option<String>, Option<String>, String) = {
            let reader = state.db.reader().unwrap();
            reader
                .query_row(
                    "SELECT display_name, public_name, name_state FROM account",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap()
        };
        assert_eq!(display, None, "the holder's copy is gone");
        assert_eq!(public_name, None, "and so is the published one");
        assert_eq!(state_column, "erased");
        assert!(
            name::pending(&state.db, 10).unwrap().is_empty(),
            "a withdrawn name must not sit in front of a reviewer"
        );
    }

    /// The holder's half of FR-036: erasing the name costs the account nothing it had earned. The
    /// published side of the same promise — the chain, the entries and the aggregate — is checked
    /// from outside the process in `tests/erasure.rs`, which is where T074 belongs.
    #[tokio::test]
    async fn erasure_costs_the_account_none_of_its_record() {
        let mut fixture = Fixture::new();
        let player = fixture.player();
        let state = fixture.state.clone();
        name::approve(&state.db, &player.id, &player.name).unwrap();
        fixture.play(&player, 12, 3);

        let mine_before =
            json(call(&state, authenticated("GET", "/api/stats/me", &player.token)).await).await;

        let response = call(
            &state,
            authenticated("DELETE", "/api/account/name", &player.token),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        assert_eq!(
            json(call(&state, authenticated("GET", "/api/stats/me", &player.token)).await).await,
            mine_before,
            "every figure the holder had before the erasure is the figure they have after it"
        );
        // Still playable, which is the difference between erasing a name and closing an account.
        assert_eq!(
            crate::account::authenticate(&state.db, &player.token).unwrap(),
            Some(player.id)
        );
    }

    /// Idempotent by the contract. A `DELETE` asks for a state, not for an event, and a second
    /// click on a slow connection must not produce an error page about a name that is already gone.
    #[tokio::test]
    async fn erasing_twice_is_not_an_error() {
        let state = test_support::state();
        let created = json(call(&state, create_request("203.0.113.31", "otherfren")).await).await;
        let token = created["access_token"].as_str().unwrap().to_string();

        for _ in 0..2 {
            let response = call(&state, authenticated("DELETE", "/api/account/name", &token)).await;
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
        }
    }

    /// The token is the only proof of ownership there is (D9), so a request without one is not a
    /// request about anybody's name.
    #[tokio::test]
    async fn a_stranger_cannot_erase_a_name() {
        let state = test_support::state();
        let created = json(call(&state, create_request("203.0.113.32", "otherfren")).await).await;

        for token in ["not-a-token", ""] {
            let response = call(&state, authenticated("DELETE", "/api/account/name", token)).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        let response = call(
            &state,
            Request::builder()
                .method("DELETE")
                .uri("/api/account/name")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        assert_eq!(
            crate::account::own(
                &state.db,
                &crate::account::authenticate(&state.db, created["access_token"].as_str().unwrap())
                    .unwrap()
                    .unwrap()
            )
            .unwrap()
            .unwrap()
            .name
            .as_deref(),
            Some("otherfren"),
            "the name nobody was authorised to erase is still there"
        );
    }

    /// What the world sees afterwards. The row stays on the board — an account that vanished would
    /// be a record nobody can check (FR-036) — masked exactly like a name awaiting approval
    /// (FR-047) and still attributable through the public identifier beside it (FR-029). The holder
    /// gets `null` and the state, so the client can say *why* there is no name rather than offering
    /// a field that will never be accepted again.
    #[tokio::test]
    async fn after_erasure_the_board_shows_the_mask_and_the_holder_sees_no_name() {
        // The shipped eligibility floor is a hundred trials; lowered so the board can be populated
        // without writing a hundred log entries. The day count, which carries the argument, stands.
        let mut config = Config::default();
        config.thresholds.eligibility_trials = config.thresholds.stats_unlock_at;
        let mut fixture = Fixture::with_config(config);
        let player = fixture.player();
        let state = fixture.state.clone();
        name::approve(&state.db, &player.id, &player.name).unwrap();
        fixture.play_across_days(&player, 4, 3, 3);

        let before = public(&state, "/api/leaderboard").await;
        assert_eq!(before["entries"][0]["name"], player.name);

        call(
            &state,
            authenticated("DELETE", "/api/account/name", &player.token),
        )
        .await;

        let after = public(&state, "/api/leaderboard").await;
        assert_eq!(
            after["entries"][0]["public_id"], player.public_id,
            "the account is still on the board and still checkable against the log"
        );
        assert_eq!(after["entries"][0]["name"], MASK);
        assert_eq!(
            after["entries"][0]["completed"], before["entries"][0]["completed"],
            "with the record it earned"
        );

        let whoami =
            json(call(&state, authenticated("GET", "/api/account", &player.token)).await).await;
        assert!(whoami["name"].is_null());
        assert_eq!(whoami["name_state"], "erased");
    }

    /// State whose cooldown has already elapsed, so a rename is reachable in one request.
    ///
    /// The first name is a submission like any other and starts the clock, so with the shipped
    /// twenty-four hours every rename test would otherwise have to backdate a column. The wait
    /// itself is what `a_rename_inside_the_cooldown_is_refused_with_the_wait` covers.
    fn state_without_cooldown() -> AppState {
        let config = Config {
            rename_cooldown_hours: 0,
            ..Config::default()
        };
        let mut state = test_support::state();
        state.config = std::sync::Arc::new(config);
        state
    }

    /// FR-048, and the half of D25 that makes a rename safe to offer: the name the board already
    /// carries stays on it while the new one is reviewed. A rename that blanked the row would
    /// charge the holder anonymity for changing their mind.
    #[tokio::test]
    async fn a_rename_queues_the_new_name_and_leaves_the_published_one_up() {
        let state = state_without_cooldown();
        let token = approved(&state, "203.0.113.40", "otherfren").await;

        let response = call(&state, rename_request(&token, "  Monroe   Institut ")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let json = json(response).await;
        // Stored form, not the typed one, exactly as creation answers.
        assert_eq!(json["name"], "Monroe Institut");
        assert_eq!(json["name_state"], "pending");

        let public_name: Option<String> = state
            .db
            .reader()
            .unwrap()
            .query_row("SELECT public_name FROM account", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            public_name.as_deref(),
            Some("otherfren"),
            "the approved name stays up until the replacement clears"
        );
    }

    /// The rate limit of FR-048. `429` and not `400`: nothing was wrong with the name, the request
    /// was early, and `Retry-After` is how long.
    #[tokio::test]
    async fn a_rename_inside_the_cooldown_is_refused_with_the_wait() {
        let state = test_support::state();
        let token = approved(&state, "203.0.113.41", "otherfren").await;

        let response = call(&state, rename_request(&token, "someoneelse")).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry_after: i64 = response
            .headers()
            .get("retry-after")
            .expect("a wait the client can act on")
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(retry_after > 0 && retry_after <= 24 * 3600);

        let json = json(response).await;
        assert_eq!(json["error"], "too_soon");
        assert_eq!(json["retry_after_seconds"], retry_after);

        let display: Option<String> = state
            .db
            .reader()
            .unwrap()
            .query_row("SELECT display_name FROM account", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            display.as_deref(),
            Some("otherfren"),
            "a refused rename changed nothing"
        );
    }

    /// The clause T100 exists to confirm: a name the pre-filter refuses does not spend the turn.
    /// The filter runs before the transaction, so nothing is written and the clock is untouched —
    /// a user who typed something the filter dislikes is not locked out for a day over it.
    #[tokio::test]
    async fn a_refused_name_does_not_consume_the_cooldown() {
        let state = state_without_cooldown();
        let token = approved(&state, "203.0.113.42", "otherfren").await;

        let refused = call(&state, rename_request(&token, "h1tl3r")).await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
        assert_eq!(json(refused).await["error"], "hate");

        let accepted = call(&state, rename_request(&token, "someoneelse")).await;
        assert_eq!(
            accepted.status(),
            StatusCode::OK,
            "the turn was still there to take"
        );
        assert_eq!(json(accepted).await["name"], "someoneelse");
    }

    /// A rejection clears the clock for the same reason: the holder has not had a turn yet, so the
    /// reviewer's refusal must not also cost them the day. This is the path through the review
    /// queue rather than through the pre-filter.
    #[tokio::test]
    async fn a_rejected_name_does_not_consume_the_cooldown_either() {
        let state = test_support::state();
        let created = json(call(&state, create_request("203.0.113.43", "otherfren")).await).await;
        let token = created["access_token"].as_str().unwrap().to_string();
        let id = crate::account::authenticate(&state.db, &token)
            .unwrap()
            .unwrap();
        name::reject(&state.db, &id, "otherfren", "hate").unwrap();

        let response = call(&state, rename_request(&token, "someoneelse")).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "the submission the reviewer refused did not count as the holder's turn"
        );
    }

    /// Erasure is permanent (FR-035), so it is not a state a rename walks back out of. The client
    /// is told which refusal this is, so it can say why there is no field rather than offering one
    /// that will never be accepted.
    #[tokio::test]
    async fn an_erased_name_cannot_be_renamed_back() {
        let state = state_without_cooldown();
        let token = approved(&state, "203.0.113.44", "otherfren").await;
        call(&state, authenticated("DELETE", "/api/account/name", &token)).await;

        let response = call(&state, rename_request(&token, "someoneelse")).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(json(response).await["error"], "erased");
    }

    /// The token is the only proof of ownership there is (D9). A request without one is not a
    /// request about anybody's name — the same rule the erasure route holds to.
    #[tokio::test]
    async fn a_stranger_cannot_rename_somebody() {
        let state = state_without_cooldown();
        let token = approved(&state, "203.0.113.45", "otherfren").await;

        let response = call(&state, rename_request("not-a-token", "someoneelse")).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let whoami = json(call(&state, authenticated("GET", "/api/account", &token)).await).await;
        assert_eq!(whoami["name"], "otherfren");
    }
}
