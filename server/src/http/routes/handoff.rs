//! `POST /api/handoff` and `POST /api/handoff/redeem` — the language switch (D11).
//!
//! Two requests on two domains, against one database (D24). The origin domain mints a code for the
//! account already authenticated there; the target domain exchanges it for a token of its own and
//! burns it. Nothing crosses between the processes but the code, and the code is worth thirty
//! seconds.

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::post;
use serde::{Deserialize, Serialize};

use crate::account::handoff;
use crate::db::now_rfc3339;
use crate::http::routes::account::Holder;
use crate::http::{ApiError, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/handoff", post(mint))
        .route("/api/handoff/redeem", post(redeem))
}

#[derive(Serialize)]
struct MintResponse {
    /// Goes into `#h=<code>` on the other domain. A fragment, like the access token itself, so the
    /// code is never transmitted to the target server in a request line and never lands in its
    /// history or in an access log (D9, FR-006).
    code: String,
    /// Reported rather than assumed, so the client can say how long the link is good for instead
    /// of hard-coding a number that would silently disagree with the server.
    expires_in: i64,
}

/// Mints a code for the authenticated account.
async fn mint(
    State(state): State<AppState>,
    Holder(account): Holder,
) -> Result<Response, ApiError> {
    let code = handoff::mint(&state.db, &account, &now_rfc3339())?;
    Ok((
        StatusCode::CREATED,
        Json(MintResponse {
            code,
            expires_in: handoff::LIFETIME_SECONDS,
        }),
    )
        .into_response())
}

#[derive(Deserialize)]
struct RedeemRequest {
    code: String,
}

#[derive(Serialize)]
struct RedeemResponse {
    /// A **new** token: the one the user already holds cannot be returned, because only its hash
    /// was ever stored (D9). See [`handoff::redeem`] — the origin domain's copy stops working
    /// here, which is the price of not keeping a usable credential in the database.
    access_token: String,
}

/// Exchanges a code for an access token.
///
/// Unauthenticated, necessarily: this is how the caller becomes authenticated on this domain. The
/// code is the credential, which is why it is 128 bits from a CSPRNG rather than something short
/// enough to guess inside its own thirty-second window.
///
/// No rate limit, and that is a decision rather than an omission. Guessing is not the attack the
/// numbers permit, and a per-address limit here would let one office behind one NAT gateway spend
/// each other's language switches — a real failure, traded against an imaginary one.
async fn redeem(
    State(state): State<AppState>,
    Json(request): Json<RedeemRequest>,
) -> Result<Response, ApiError> {
    match handoff::redeem(&state.db, &request.code, &now_rfc3339())? {
        Some(access_token) => Ok(Json(RedeemResponse { access_token }).into_response()),
        // The same 410 for a code that was burnt, one that expired and one that never existed.
        // Distinguishing them would make this endpoint answer "was that ever a real code", which
        // is the one question an attacker holding a guess would like answered.
        None => Err(ApiError::Gone),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::account;
    use crate::db::now_rfc3339;
    use crate::http::{AppState, router, test_support};

    fn post(uri: &str, token: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let mut request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        request.body(Body::from(body.to_string())).unwrap()
    }

    async fn call(state: &AppState, request: Request<Body>) -> (StatusCode, serde_json::Value) {
        let response = router(state.clone()).oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    fn holder(state: &AppState) -> String {
        account::create(&state.db, "otherfren", &now_rfc3339())
            .expect("the fixture name passes the filter")
            .access_token
    }

    #[tokio::test]
    async fn a_code_is_minted_and_redeemed_for_a_working_token() {
        let state = test_support::state();
        let token = holder(&state);

        let (status, body) = call(&state, post("/api/handoff", Some(&token), json_empty())).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["expires_in"], 30);
        let code = body["code"].as_str().unwrap().to_string();

        let (status, body) = call(
            &state,
            post(
                "/api/handoff/redeem",
                None,
                serde_json::json!({ "code": code }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let switched = body["access_token"].as_str().unwrap();
        assert!(
            account::authenticate(&state.db, switched)
                .unwrap()
                .is_some()
        );
    }

    /// D11's whole point is that the long-lived token never travels. The mint response must
    /// therefore carry the code and nothing that could stand in for the token.
    #[tokio::test]
    async fn minting_hands_over_nothing_but_the_code() {
        let state = test_support::state();
        let token = holder(&state);

        let (_, body) = call(&state, post("/api/handoff", Some(&token), json_empty())).await;
        let mut keys: Vec<&str> = body
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["code", "expires_in"]);
        assert!(
            !body.to_string().contains(&token),
            "the access token must not ride along with the code"
        );
    }

    #[tokio::test]
    async fn a_burnt_or_unknown_code_is_gone() {
        let state = test_support::state();
        let token = holder(&state);
        let (_, body) = call(&state, post("/api/handoff", Some(&token), json_empty())).await;
        let code = body["code"].as_str().unwrap().to_string();

        let redeem = |code: serde_json::Value| post("/api/handoff/redeem", None, code);
        assert_eq!(
            call(&state, redeem(serde_json::json!({ "code": code })))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            call(&state, redeem(serde_json::json!({ "code": code })))
                .await
                .0,
            StatusCode::GONE,
            "single use"
        );
        assert_eq!(
            call(&state, redeem(serde_json::json!({ "code": "nonsense" })))
                .await
                .0,
            StatusCode::GONE,
            "an unknown code must look exactly like a burnt one"
        );
    }

    #[tokio::test]
    async fn minting_is_closed_to_strangers() {
        let state = test_support::state();
        let (status, _) = call(&state, post("/api/handoff", None, json_empty())).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    fn json_empty() -> serde_json::Value {
        serde_json::json!({})
    }
}
