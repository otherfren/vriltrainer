//! The harness the contract tests share: a service, a way to call it, and a way to play a trial.
//!
//! Every file under `server/tests` is a stranger to the process it is testing — it goes in through
//! `router()` and reads what comes back, and nothing here hands a test a database handle or an
//! in-memory figure. That is the difference between these and the `#[cfg(test)]` modules inside
//! the route files: those check that a function does what it says, and these check that the
//! contract in `specs/001-remote-viewing-trainer/contracts/` is what the wire carries.
//!
//! Kept in one place because four contract files that each built their own fixture would drift,
//! and a fixture that differs between two contract tests is a fixture that makes one of them a
//! test of itself.

#![allow(dead_code)] // Each contract file uses a different part of this.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use tower::ServiceExt;

use server::account;
use server::config::Config;
use server::db::{Db, now_rfc3339};
use server::http::{AppState, router};
use server::pool::{ImageEntry, Manifest};
use server::trial::token::Sealer;

/// A service with a pool large enough to draw from: ten categories of three images.
pub fn service() -> AppState {
    service_with(Config {
        // A test cannot wait out the real minimum viewing time, and none of these tests is about
        // the timing gate — which has its own tests next to the handler.
        min_view_seconds: 0,
        ..Config::default()
    })
}

/// The same service over a configuration the test chose. Thresholds are the usual reason: the
/// shipped eligibility floor is a hundred trials, and a contract test that had to write a hundred
/// entries to see a leaderboard row would be testing patience.
pub fn service_with(config: Config) -> AppState {
    let locale = config.locale;
    let categories: Vec<String> = (0..10).map(|c| format!("cat{c}")).collect();
    let images: Vec<ImageEntry> = (0..30)
        .map(|i| ImageEntry {
            // Zero-padded, because the manifest is sorted ascending by id and that order *is* the
            // index the derivation draws against.
            id: format!("img_{i:03}"),
            category: format!("cat{}", i / 3),
        })
        .collect();
    let pool = Manifest {
        version: 1,
        manifest_hash: Manifest::compute_hash(&categories, &images),
        categories,
        images,
    };
    pool.validate().expect("the fixture is a valid manifest");

    AppState {
        db: Arc::new(Db::open_in_memory().expect("an in-memory database opens")),
        config: Arc::new(config),
        sealer: Arc::new(Sealer::new(&[11u8; 32])),
        pool: Arc::new(pool),
        metrics: Arc::new(server::metrics::Metrics::new(locale, &now_rfc3339())),
    }
}

pub async fn call(state: &AppState, request: Request<Body>) -> axum::response::Response {
    router(state.clone()).oneshot(request).await.unwrap()
}

pub fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

pub fn authed(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

pub fn post(uri: &str, token: &str, body: serde_json::Value) -> Request<Body> {
    with_body("POST", uri, Some(token), body)
}

pub fn with_body(
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: serde_json::Value,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

pub async fn body_text(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

pub async fn body_json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_str(&body_text(response).await).unwrap()
}

/// An account created the way the contract says accounts are created: over HTTP.
pub async fn create_account(state: &AppState, name: &str) -> serde_json::Value {
    let response = call(
        state,
        with_body("POST", "/api/account", None, json!({ "name": name })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body_json(response).await
}

/// An account created directly, for tests that want a token and not a contract check on the
/// creation route.
pub fn account_token(state: &AppState, name: &str) -> String {
    account::create(&state.db, name, &now_rfc3339())
        .expect("the fixture names pass the filter")
        .access_token
}

/// The same, when the test also needs the opaque identifier — the log carries it, and so does the
/// sealed token.
pub fn account_id_and_token(state: &AppState, name: &str) -> (String, String) {
    let created = account::create(&state.db, name, &now_rfc3339())
        .expect("the fixture names pass the filter");
    (created.id, created.access_token)
}

/// What the participant's own browser saw, kept so the record can be checked against it.
pub struct Played {
    pub trial_id: String,
    /// The eight identifiers as displayed, in order.
    pub images: Vec<String>,
    /// `None` for a trial that was abandoned after the reveal.
    pub outcome: Option<Outcome>,
}

pub struct Outcome {
    pub hit: bool,
    pub target: String,
    pub seq: u64,
}

/// Start, reveal, and answer unless `abandon`.
pub async fn play(state: &AppState, token: &str, s_client: &[u8; 32], abandon: bool) -> Played {
    let start = call(state, post("/api/trial", token, json!({}))).await;
    assert_eq!(start.status(), StatusCode::CREATED);
    let start = body_json(start).await;
    let trial_id = start["trial_id"].as_str().unwrap().to_string();

    let reveal = call(
        state,
        post(
            "/api/trial/reveal",
            token,
            json!({ "token": start["token"], "s_client": STANDARD.encode(s_client) }),
        ),
    )
    .await;
    assert_eq!(reveal.status(), StatusCode::OK);
    let reveal = body_json(reveal).await;
    let images: Vec<String> = reveal["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    if abandon {
        // No further request. Abandonment needs no marker and no sweep — it is the absence of a
        // resolve entry, which is what makes it countable by a stranger (FR-027).
        return Played {
            trial_id,
            images,
            outcome: None,
        };
    }

    let answer = call(
        state,
        post(
            "/api/trial/answer",
            token,
            json!({ "token": reveal["token"], "chosen": images[0] }),
        ),
    )
    .await;
    assert_eq!(answer.status(), StatusCode::OK);
    let answer = body_json(answer).await;

    Played {
        trial_id,
        images,
        outcome: Some(Outcome {
            hit: answer["hit"].as_bool().unwrap(),
            target: answer["target"].as_str().unwrap().to_string(),
            seq: answer["seq"].as_u64().unwrap(),
        }),
    }
}

/// `answered` trials played to the end and `abandoned` walked away from, for one account.
pub async fn play_many(
    state: &AppState,
    token: &str,
    tag: u8,
    answered: usize,
    abandoned: usize,
) -> Vec<Played> {
    let mut all = Vec::new();
    // Distinct client randomness per trial, so no two trials share a seed and a check that passed
    // by accident on a repeated draw would show up as a duplicate target.
    for t in 0..(answered + abandoned) {
        let mut s_client = [0u8; 32];
        s_client[0] = tag;
        s_client[1] = t as u8;
        all.push(play(state, token, &s_client, t >= answered).await);
    }
    all
}
