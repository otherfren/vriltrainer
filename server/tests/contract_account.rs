//! Conformance against `contracts/http-api.md`, sections "Account" and "Language handoff".
//!
//! Constitution principle III. The account routes had tests, but they were `#[cfg(test)]` modules
//! inside the route files — they check the handlers, and what was missing was a check on the
//! contract: the shapes an integrator is promised, and the four refusals that are the whole of the
//! error surface here.

use axum::http::{StatusCode, header};
use serde_json::{Value, json};

use server::account::name;
use server::config::Config;

mod common;
use common::*;

fn service_now() -> server::http::AppState {
    service_with(Config {
        min_view_seconds: 0,
        // The first name counts as the first submission, so the shipped day-long cooldown would
        // make every rename in this file a 429. The cooldown has its own test below, with the
        // shipped value.
        rename_cooldown_hours: 0,
        ..Config::default()
    })
}

async fn whoami(state: &server::http::AppState, token: &str) -> Value {
    let response = call(state, authed("GET", "/api/account", token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await
}

/// `POST /api/account`: the token is returned once and never again, and the name comes back in the
/// **stored** form rather than the typed one — a client that displays what was typed displays a
/// name the server does not hold.
#[tokio::test]
async fn creating_an_account_returns_the_stored_name_and_the_token_once() {
    let state = service_now();
    let created = create_account(&state, "  Monroe   Institut ").await;

    assert_eq!(created["name"], "Monroe Institut");
    let public_id = created["public_id"].as_str().unwrap();
    assert_eq!(public_id.len(), 6);
    assert!(
        public_id
            .chars()
            .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c)),
        "six uppercase hex characters, not {public_id}"
    );
    assert!(!created["access_token"].as_str().unwrap().is_empty());

    // Not rate-limited: the per-address cap of D17 was removed by D30.
    for _ in 0..5 {
        create_account(&state, "otherfren").await;
    }
}

/// The refusal carries the machine-readable code, never a sentence: the sentence is product copy,
/// it differs per domain (D10), and it lives in the client's message catalogue.
#[tokio::test]
async fn a_refused_name_is_a_four_hundred_carrying_its_code() {
    let state = service_now();
    let response = call(
        &state,
        with_body("POST", "/api/account", None, json!({ "name": "h1tl3r" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body["error"], "hate");
    assert_eq!(body.as_object().unwrap().len(), 1, "a code, not a sentence");
}

/// `GET /api/account` exists because an access link is a capability and carries no identity (D9):
/// a browser that arrived through one holds a token and knows nothing about whose account it opens.
/// The holder is not a stranger and is never shown the mask (D25, FR-047).
#[tokio::test]
async fn the_holder_is_shown_their_own_name_in_whatever_state_it_is_in() {
    let state = service_now();
    let created = create_account(&state, "otherfren").await;
    let token = created["access_token"].as_str().unwrap();

    let me = whoami(&state, token).await;
    assert_eq!(me["public_id"], created["public_id"]);
    assert_eq!(me["name"], "otherfren");
    assert_eq!(me["name_state"], "pending", "nothing is public unreviewed");
    assert_ne!(me["name"], name::MASK, "the holder never sees the mask");

    assert_eq!(
        call(&state, authed("GET", "/api/account", "not-a-token"))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

/// `PUT /api/account/name` (FR-048). Always `pending`, because every name goes through review
/// including a replacement for one that was already approved — and the approved one stays on the
/// board while the replacement is looked at, so a rename is not punished with anonymity.
#[tokio::test]
async fn a_rename_queues_the_new_name_and_the_approved_one_stays_published() {
    let state = service_now();
    let created = create_account(&state, "otherfren").await;
    let token = created["access_token"].as_str().unwrap().to_string();
    let id = server::account::authenticate(&state.db, &token)
        .unwrap()
        .unwrap();
    name::approve(&state.db, &id, "otherfren").unwrap();

    let response = call(
        &state,
        with_body(
            "PUT",
            "/api/account/name",
            Some(&token),
            json!({ "name": "  Monroe  Institut " }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["name"], "Monroe Institut", "the stored form");
    assert_eq!(body["name_state"], "pending");
    assert_eq!(body["public_id"], created["public_id"]);

    let published: Option<String> = state
        .db
        .reader()
        .unwrap()
        .query_row(
            "SELECT public_name FROM account WHERE id = ?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(published.as_deref(), Some("otherfren"));
}

/// The rate limit, with the shipped twenty-four hours. `429` and not `400`: nothing was wrong with
/// the name, the request was early, and `Retry-After` is how long — the header a script already
/// knows how to read.
#[tokio::test]
async fn a_rename_inside_the_cooldown_is_four_twenty_nine_with_the_wait() {
    let state = service_with(Config {
        min_view_seconds: 0,
        ..Config::default()
    });
    let created = create_account(&state, "otherfren").await;
    let token = created["access_token"].as_str().unwrap();

    let response = call(
        &state,
        with_body(
            "PUT",
            "/api/account/name",
            Some(token),
            json!({ "name": "someoneelse" }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let wait: i64 = response
        .headers()
        .get(header::RETRY_AFTER)
        .expect("a wait the client can act on")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=24 * 3600).contains(&wait));

    let body = body_json(response).await;
    assert_eq!(body["error"], "too_soon");
    assert_eq!(body["retry_after_seconds"], wait);
    assert_eq!(whoami(&state, token).await["name"], "otherfren");
}

/// A refusal does not consume the cooldown: the user has not had a turn yet.
#[tokio::test]
async fn a_refused_rename_leaves_the_turn_unspent() {
    let state = service_now();
    let token = create_account(&state, "otherfren")
        .await
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();

    let refused = call(
        &state,
        with_body(
            "PUT",
            "/api/account/name",
            Some(&token),
            json!({ "name": "h1tl3r" }),
        ),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(refused).await["error"], "hate");

    let accepted = call(
        &state,
        with_body(
            "PUT",
            "/api/account/name",
            Some(&token),
            json!({ "name": "someoneelse" }),
        ),
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::OK);
}

/// `DELETE /api/account/name`: `204`, idempotent, and the trials stay in the log under the opaque
/// identifier (FR-035, FR-036). Erasure is permanent, so a rename afterwards is refused with the
/// code that says which refusal it is.
#[tokio::test]
async fn erasing_a_name_is_two_oh_four_idempotent_and_final() {
    let state = service_now();
    let token = create_account(&state, "otherfren")
        .await
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    play_many(&state, &token, 0, 3, 0).await;
    let before = body_json(call(&state, get("/api/log/head")).await).await;

    for _ in 0..2 {
        let response = call(&state, authed("DELETE", "/api/account/name", &token)).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(body_text(response).await.is_empty());
    }

    let me = whoami(&state, &token).await;
    assert!(me["name"].is_null());
    assert_eq!(me["name_state"], "erased");

    // The record is untouched: erasure costs the log nothing, which is what FR-026 bought by
    // keeping names out of the chain in the first place.
    let after = body_json(call(&state, get("/api/log/head")).await).await;
    assert_eq!(after["seq"], before["seq"]);
    assert_eq!(after["entry_hash"], before["entry_hash"]);

    let rename = call(
        &state,
        with_body(
            "PUT",
            "/api/account/name",
            Some(&token),
            json!({ "name": "someoneelse" }),
        ),
    )
    .await;
    assert_eq!(rename.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(rename).await["error"], "erased");
}

/// The handoff is how a session crosses the origin boundary between the two domains without the
/// long-lived token ever entering an address bar (FR-031, D11). The token that comes back is a
/// **new** one and the previous one stops working — forced rather than chosen, because only a hash
/// of an access token is ever stored (D9).
#[tokio::test]
async fn a_handoff_code_is_burnt_for_a_new_token_and_the_old_one_dies() {
    let state = service_now();
    let created = create_account(&state, "otherfren").await;
    let old = created["access_token"].as_str().unwrap().to_string();

    let issued = call(&state, post("/api/handoff", &old, json!({}))).await;
    assert_eq!(issued.status(), StatusCode::CREATED);
    let issued = body_json(issued).await;
    let code = issued["code"].as_str().expect("a code").to_string();
    assert_eq!(
        issued["expires_in"], 30,
        "short-lived, as the contract says"
    );

    let redeemed = call(
        &state,
        with_body("POST", "/api/handoff/redeem", None, json!({ "code": code })),
    )
    .await;
    assert_eq!(redeemed.status(), StatusCode::OK);
    let new = body_json(redeemed).await["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    assert_ne!(new, old, "one account holds one live token");
    assert_eq!(
        whoami(&state, &new).await["public_id"],
        created["public_id"],
        "the same account, reached by the new token"
    );
    assert_eq!(
        call(&state, authed("GET", "/api/account", &old))
            .await
            .status(),
        StatusCode::UNAUTHORIZED,
        "the previous token stopped working"
    );
}

/// Used, expired and never issued are deliberately indistinguishable: all three are `410`, so the
/// response cannot be used to find out whether a guessed code ever existed.
#[tokio::test]
async fn a_spent_code_and_an_invented_one_are_the_same_gone() {
    let state = service_now();
    let token = create_account(&state, "otherfren")
        .await
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let code = body_json(call(&state, post("/api/handoff", &token, json!({}))).await).await["code"]
        .as_str()
        .unwrap()
        .to_string();

    // The token from the first redemption is discarded: this test is about the second attempt.
    assert_eq!(
        call(
            &state,
            with_body("POST", "/api/handoff/redeem", None, json!({ "code": code })),
        )
        .await
        .status(),
        StatusCode::OK
    );

    for attempt in [code.as_str(), "never-issued-at-all"] {
        let response = call(
            &state,
            with_body(
                "POST",
                "/api/handoff/redeem",
                None,
                json!({ "code": attempt }),
            ),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::GONE,
            "{attempt} was answered differently from the other case"
        );
    }
}

/// Issuing a code is authenticated — it is the account's own credential being handed across, and
/// an unauthenticated issue endpoint would mint a session for whoever asked.
#[tokio::test]
async fn a_stranger_cannot_have_a_handoff_code() {
    let state = service_now();
    assert_eq!(
        call(&state, post("/api/handoff", "not-a-token", json!({})))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}
