//! Conformance against `contracts/http-api.md`, `GET /api/leaderboard`, and against
//! `contracts/pool-manifest.md` for the manifest a verifier needs alongside it.
//!
//! Constitution principle III. Until this file the board had unit tests inside its own route module
//! and nothing that went in through the wire — so the paging contract, the mask, and the promise
//! that `by_chance_per_10k` can be recomputed from the two counts printed beside it were checked
//! only by the code that produces them.

use axum::http::{StatusCode, header};
use serde_json::Value;

use server::account::name;
use server::config::{Config, Thresholds};

mod common;
use common::*;

/// Eligibility inside what an HTTP test can build: ten trials on the one day it can reach.
fn short_record() -> Config {
    Config {
        min_view_seconds: 0,
        thresholds: Thresholds {
            stats_unlock_at: 10,
            eligibility_trials: 10,
            eligibility_days: 1,
            ..Thresholds::default()
        },
        ..Config::default()
    }
}

/// `players` accounts on the board with an approved name, plus one that has played too little and
/// belongs in the queue behind it.
async fn populated(players: u8) -> server::http::AppState {
    let state = service_with(short_record());
    for a in 0..players {
        let name = format!("viewer{a}");
        let (id, token) = account_id_and_token(&state, &name);
        name::approve(&state.db, &id, &name).expect("the reviewer approves the fixture names");
        play_many(&state, &token, a, 10, 0).await;
    }
    let waiting = account_token(&state, "newcomer");
    play_many(&state, &waiting, 200, 3, 0).await;
    state
}

async fn board(state: &server::http::AppState, query: &str) -> Value {
    let response = call(state, get(&format!("/api/leaderboard{query}"))).await;
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await
}

/// The envelope: what the board is over, what the ladder is called, and when the ranks were last
/// computed — the last of these because a rank that has not moved otherwise reads as a bug.
#[tokio::test]
async fn the_board_reports_what_it_is_over_and_when_it_was_computed() {
    let state = populated(2).await;
    let body = board(&state, "").await;

    assert_eq!(body["eligible_accounts"], 2);
    // Reported whether or not anybody holds a title (FR-042), and the full ladder since D31 —
    // readers use it to see what the rungs above them are called.
    let bands = body["bands_active"]
        .as_array()
        .expect("the ladder is named");
    assert_eq!(bands.len(), 5, "nearest the middle first, all of them");
    assert_eq!(bands[0], "asset");
    assert!(
        body["ranks_updated_at"].as_str().unwrap().ends_with('Z'),
        "an RFC 3339 instant in UTC"
    );
    assert!(body["thresholds"].is_object());
}

/// Paging: the defaults are echoed so a client can page without keeping its own copy of them, and
/// `limit` is clamped rather than obeyed — an unbounded page is a denial-of-service parameter.
#[tokio::test]
async fn paging_echoes_its_defaults_and_clamps_the_limit() {
    let state = populated(3).await;

    let defaults = board(&state, "").await;
    assert_eq!(defaults["offset"], 0);
    assert_eq!(defaults["limit"], 20);

    let asked = board(&state, "?offset=1&limit=1").await;
    assert_eq!(asked["offset"], 1);
    assert_eq!(asked["limit"], 1);
    assert_eq!(asked["entries"].as_array().unwrap().len(), 1);
    assert_ne!(
        asked["entries"][0]["public_id"], defaults["entries"][0]["public_id"],
        "an offset of one skips the first row"
    );

    let clamped = board(&state, "?limit=1000").await;
    assert_eq!(clamped["limit"], 100);
    let floored = board(&state, "?limit=0").await;
    assert_eq!(floored["limit"], 1);
}

/// Every entry carries both sort keys and the supporting counts (FR-041, D20). A board sorted on
/// something it does not show is the complaint D20 settled.
#[tokio::test]
async fn every_entry_carries_the_figures_the_order_is_made_of() {
    let state = populated(3).await;
    let body = board(&state, "").await;
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3);

    let mut previous: Option<(f64, f64)> = None;
    for (i, entry) in entries.iter().enumerate() {
        for field in [
            "place",
            "name",
            "public_id",
            "wilson_lower",
            "wilson_upper",
            "completed",
            "hits",
            "hit_rate",
            "by_chance_per_10k",
            "deviation",
            "proven",
        ] {
            assert!(!entry[field].is_null(), "{field} is missing from {entry}");
        }
        assert_eq!(entry["place"], i as u64 + 1, "places count from one");

        // Sorted by wilson_lower descending, then wilson_upper (D20).
        let keys = (
            entry["wilson_lower"].as_f64().unwrap(),
            entry["wilson_upper"].as_f64().unwrap(),
        );
        if let Some(before) = previous {
            assert!(
                before.0 > keys.0 || (before.0 == keys.0 && before.1 >= keys.1),
                "the board is out of order at place {}: {before:?} then {keys:?}",
                i + 1
            );
        }
        previous = Some(keys);

        // The column's whole claim is that a reader can reproduce it from the two counts printed
        // beside it, which is why it is computed live rather than per block.
        let (hits, completed) = (
            entry["hits"].as_f64().unwrap(),
            entry["completed"].as_f64().unwrap(),
        );
        assert!((entry["hit_rate"].as_f64().unwrap() - hits / completed).abs() < 1e-9);

        // `proven` is the server's single definition of "more than luck", so two implementations
        // of the boundary cannot disagree in public.
        assert_eq!(
            entry["proven"].as_bool().unwrap(),
            entry["wilson_lower"].as_f64().unwrap() > 0.125
        );
    }
}

/// `name` is the most recently approved name or a fixed-length mask, and `public_id` is beside it
/// either way — so a masked row is still attributable and still checkable against the log
/// (FR-047, FR-029, D25). The mask reveals neither the length nor the characters of what it hides.
#[tokio::test]
async fn an_unapproved_name_is_masked_and_the_row_stays_attributable() {
    let state = service_with(short_record());
    let long = "Monroe Institut";
    let (_, token) = account_id_and_token(&state, long);
    play_many(&state, &token, 0, 10, 0).await;

    let body = board(&state, "").await;
    let entry = &body["entries"][0];
    assert_eq!(entry["name"], name::MASK);
    assert_ne!(entry["name"].as_str().unwrap().chars().count(), long.len());
    assert_eq!(entry["public_id"].as_str().unwrap().len(), 6);
    assert_eq!(entry["completed"], 10, "with the record it earned");
}

/// The queue behind the board answers "is anything happening here", and that question is asked
/// once, at the top — so `waiting` is sent with the first page only. No rate is reported for a
/// queue row: those records are below the floor by definition.
#[tokio::test]
async fn the_waiting_queue_comes_with_the_first_page_only() {
    let state = populated(2).await;

    let first = board(&state, "?offset=0").await;
    let waiting = first["waiting"]
        .as_array()
        .expect("the queue is on page one");
    assert_eq!(waiting.len(), 1);
    assert_eq!(first["waiting_accounts"], 1);

    let row = &waiting[0];
    assert_eq!(row["completed"], 3);
    assert_eq!(row["trials_needed"], 7, "what is still outstanding");
    assert_eq!(
        row["days_needed"], 0,
        "that half of the rule is already met"
    );
    assert!(
        row["hit_rate"].is_null() && row["wilson_lower"].is_null(),
        "a rate printed against three trials is the most misread number the site could publish"
    );

    let second = board(&state, "?offset=1").await;
    assert!(
        second["waiting"].is_null() || second["waiting"].as_array().unwrap().is_empty(),
        "the queue is not repeated on every page"
    );
    // The population figure is still reported, because it is about the queue and not about a page.
    assert_eq!(second["waiting_accounts"], 1);
}

/// The manifest a verifier needs in order to recompute any trial the board is made of. The version
/// is a pointer and can be re-cut (D34), so the hash is what a commit entry is actually checked
/// against.
#[tokio::test]
async fn the_pool_manifest_is_served_by_version_and_hashes_to_what_it_says() {
    let state = service_with(short_record());

    let response = call(&state, get("/api/pool/1/manifest")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let manifest: server::pool::Manifest =
        serde_json::from_str(&body_text(response).await).unwrap();

    assert_eq!(manifest.version, 1);
    manifest
        .validate()
        .expect("the published manifest validates against its own hash");
    assert_eq!(manifest.manifest_hash, state.pool.manifest_hash);
    // Ascending by id, because that order *is* the index the derivation draws against.
    let mut sorted = manifest.images.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(sorted, manifest.images);

    // A version nobody published is a 404, not an empty manifest — an empty one would let a
    // verifier "check" a trial against nothing and conclude it was fine.
    assert_eq!(
        call(&state, get("/api/pool/99/manifest")).await.status(),
        StatusCode::NOT_FOUND
    );
}

/// The image bytes, deliberately not under `/api`: this is what an `<img src>` points at, and the
/// client builds the address from the manifest's identifiers alone. The identifier is the hash of
/// the bytes, so a cached copy cannot go stale and the caching headers say so.
#[tokio::test]
async fn an_image_is_served_immutably_and_an_unknown_one_is_a_404() {
    let state = service_with(short_record());
    // A real identifier out of the binary, not one of the fixture's synthetic ones: the bytes are
    // compiled in (D29), so this route answers for what the build carries and for nothing else.
    let (id, _) = server::pool::embedded::all()[0];

    let response = call(&state, get(&format!("/pool/{id}.png"))).await;
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/png");
    assert!(
        headers
            .get(header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("immutable")
    );
    assert!(headers.get(header::ETAG).is_some());

    assert_eq!(
        call(&state, get("/pool/img_not_a_real_id.png"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

/// The board is public. It is the site's published result, and a result behind a login is not one
/// anybody can check.
#[tokio::test]
async fn the_board_needs_no_token() {
    let state = populated(1).await;
    assert_eq!(
        call(&state, get("/api/leaderboard")).await.status(),
        StatusCode::OK
    );
}
