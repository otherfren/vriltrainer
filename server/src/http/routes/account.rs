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
use axum::routing::post;
use serde::{Deserialize, Serialize};

use crate::account::{self, name::NameError, name_filter::Refusal};
use crate::db::now_rfc3339;
use crate::http::{ApiError, AppState};

pub fn routes() -> Router<AppState> {
    // T069 and T100 — erasure and rename — mount here too, and stay 501 until they do.
    Router::new().route("/api/account", post(create).get(whoami))
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
        // Neither is reachable from creation: there is no earlier name to be inside a cooldown
        // for, and nothing to have erased. Answered rather than unwrapped, because a panic in a
        // handler is a 500 with nothing attached to explain it.
        NameError::TooSoon { .. } => ApiError::BadRequest("too_soon"),
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

    use crate::http::{router, test_support};

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
}
