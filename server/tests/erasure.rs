//! T074: erasure leaves the record verifiable (FR-035, FR-036, SC-008).
//!
//! The promise the data protection notice makes is not "your name is deleted" alone — it is that
//! deleting it costs the public record nothing. A hash chain cannot have a row taken out of it
//! afterwards, so the two claims only coexist because no name was ever in the chain (FR-026): what
//! erasure touches is one row of `account`, and every entry stays where it is under the opaque
//! identifier.
//!
//! Checked from outside, like `contract_log.rs`: the trials are played over HTTP, the record is
//! downloaded through `GET /api/log`, and the verification runs over those bytes. A test that
//! reached into the database to confirm the entries survived would be asking the server whether it
//! had kept its word.

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
use server::log::chain::{self, Body as EntryBody, Entry};
use server::log::export;
use server::pool::{ImageEntry, Manifest};
use server::trial::token::Sealer;

/// A service with a pool large enough to draw from: ten categories of three images.
fn service() -> AppState {
    let categories: Vec<String> = (0..10).map(|c| format!("cat{c}")).collect();
    let images: Vec<ImageEntry> = (0..30)
        .map(|i| ImageEntry {
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

    let config = Config {
        // The timing gate has its own tests beside the handler; this one is about the record.
        min_view_seconds: 0,
        ..Config::default()
    };

    AppState {
        db: Arc::new(Db::open_in_memory().expect("an in-memory database opens")),
        config: Arc::new(config),
        sealer: Arc::new(Sealer::new(&[13u8; 32])),
        pool: Arc::new(pool),
    }
}

async fn call(state: &AppState, request: Request<Body>) -> axum::response::Response {
    router(state.clone()).oneshot(request).await.unwrap()
}

fn post(uri: &str, token: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn body_text(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_str(&body_text(response).await).unwrap()
}

/// One trial, played to the end, and the identifier it was recorded under.
async fn play(state: &AppState, token: &str, seed: u8) -> String {
    let start = call(state, post("/api/trial", token, json!({}))).await;
    assert_eq!(start.status(), StatusCode::CREATED);
    let start = body_json(start).await;
    let trial_id = start["trial_id"].as_str().unwrap().to_string();

    let mut s_client = [0u8; 32];
    s_client[0] = seed;
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
    let chosen = reveal["images"][0].clone();

    let answer = call(
        state,
        post(
            "/api/trial/answer",
            token,
            json!({ "token": reveal["token"], "chosen": chosen }),
        ),
    )
    .await;
    assert_eq!(answer.status(), StatusCode::OK);
    trial_id
}

/// The export as a third party gets it: over HTTP, paged.
async fn download(state: &AppState) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    loop {
        let from = entries.len() as u64 + 1;
        let request = Request::builder()
            .uri(format!("/api/log?from={from}&limit=5"))
            .body(Body::empty())
            .unwrap();
        let page = call(state, request).await;
        assert_eq!(page.status(), StatusCode::OK);
        let page = export::read_ndjson(&body_text(page).await).expect("the export parses");
        if page.is_empty() {
            return entries;
        }
        entries.extend(page);
    }
}

#[tokio::test]
async fn erasing_a_name_leaves_the_published_record_intact_and_verifiable() {
    let state = service();
    let erasing = account::create(&state.db, "otherfren", &now_rfc3339())
        .expect("the fixture name passes the filter");
    let bystander = account::create(&state.db, "Monroe Institut", &now_rfc3339())
        .expect("the fixture name passes the filter");

    let mut trials = Vec::new();
    for seed in 0..3u8 {
        trials.push(play(&state, &erasing.access_token, seed).await);
        play(&state, &bystander.access_token, 100 + seed).await;
    }

    let before = download(&state).await;
    let aggregate_before = body_json(
        call(
            &state,
            Request::builder()
                .uri("/api/stats/aggregate")
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;

    let erased = call(
        &state,
        Request::builder()
            .method("DELETE")
            .uri("/api/account/name")
            .header("authorization", format!("Bearer {}", erasing.access_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(erased.status(), StatusCode::NO_CONTENT);

    // ---- The record, downloaded again and verified from the bytes alone ----------------------
    let after = download(&state).await;
    chain::verify(&after).expect("the chain still links after an erasure");
    assert_eq!(
        after, before,
        "not one entry was rewritten, resequenced or dropped"
    );

    // Every trial the account played is still there, still attributable to it — which is what
    // FR-036 promises and what makes the erasure honest rather than a quiet deletion of history.
    for trial in &trials {
        let commit = after
            .iter()
            .find(|e| e.body.trial() == trial && matches!(e.body, EntryBody::Commit { .. }))
            .expect("a trial vanished from the record");
        let EntryBody::Commit { account, .. } = &commit.body else {
            unreachable!()
        };
        assert_eq!(
            account, &erasing.id,
            "the entry is still under the opaque identifier the account keeps"
        );
        assert!(
            after
                .iter()
                .any(|e| e.body.trial() == trial && matches!(e.body, EntryBody::Resolve { .. })),
            "the outcome of trial {trial} is still published"
        );
    }

    // And the figure the site exists to report has not moved. An erasure that took trials out of
    // the aggregate would be indistinguishable, afterwards, from an operator dropping the ones
    // they did not like.
    assert_eq!(
        body_json(
            call(
                &state,
                Request::builder()
                    .uri("/api/stats/aggregate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await,
        )
        .await,
        aggregate_before
    );

    // The erased name is not in the export — nor was it before, which is the whole reason the two
    // sentences above can both be true (FR-026, D13).
    let raw = export::to_ndjson(&after);
    assert!(!raw.contains("otherfren"));
    assert!(!raw.contains("Monroe Institut"));
}
