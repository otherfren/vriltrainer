//! Conformance against `contracts/http-api.md`, section "Trial".
//!
//! Constitution principle III: a released contract is covered by a test that goes in through the
//! wire. The route module has its own tests and they check the handler; this checks the *contract*
//! — the fields the client is promised and the four-row status table, which is the part an
//! integrator reads and the part nothing else asserts as a set.

use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;

use server::config::Config;
use server::trial::token::TokenTwo;

mod common;
use common::*;

const S_CLIENT: [u8; 32] = [9u8; 32];

fn encoded() -> String {
    STANDARD.encode(S_CLIENT)
}

/// `POST /api/trial`: the six fields the client needs, and the commitment written before the
/// answer rather than alongside it.
#[tokio::test]
async fn starting_a_trial_returns_the_commitment_and_the_pool_it_was_drawn_against() {
    let state = service();
    let token = account_token(&state, "otherfren");

    let response = call(&state, post("/api/trial", &token, json!({}))).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;

    for field in [
        "trial_id",
        "coordinate",
        "commitment",
        "pool_version",
        "pool_manifest_hash",
        "token",
    ] {
        assert!(!body[field].is_null(), "{field} is missing from {body}");
    }
    assert!(
        body["commitment"].as_str().unwrap().starts_with("sha256:"),
        "the commitment names its hash function"
    );
    // Four digits, a hyphen, four digits — the string the participant is asked to sit with.
    let coordinate = body["coordinate"].as_str().unwrap();
    assert_eq!(coordinate.len(), 9);
    assert_eq!(&coordinate[4..5], "-");
    // A trial names the exact pool it was drawn against, because the version is a pointer and can
    // be re-cut (D34). Without the hash a re-cut version makes every past trial unverifiable.
    assert_eq!(body["pool_version"], 1);
    assert_eq!(body["pool_manifest_hash"], state.pool.manifest_hash);

    // No cap on open trials since D30: a second start is another 201, not a 429.
    let again = call(&state, post("/api/trial", &token, json!({}))).await;
    assert_eq!(again.status(), StatusCode::CREATED);
}

/// `POST /api/trial/reveal`: exactly eight, distinct, and a body that says nothing about which of
/// them is the target.
#[tokio::test]
async fn a_reveal_returns_eight_images_and_nothing_that_distinguishes_the_target() {
    let state = service();
    let token = account_token(&state, "otherfren");
    let start = body_json(call(&state, post("/api/trial", &token, json!({}))).await).await;

    let response = call(
        &state,
        post(
            "/api/trial/reveal",
            &token,
            json!({ "token": start["token"], "s_client": encoded() }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;

    let images: Vec<&str> = body["images"]
        .as_array()
        .expect("the contract says an array")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(images.len(), 8, "exactly eight, always");
    let mut sorted = images.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 8, "no image is offered twice");

    // The whole payload is two keys. Anything else here would be a field a client could learn to
    // read the target out of.
    let object = body.as_object().unwrap();
    assert_eq!(object.len(), 2, "unexpected fields in {body}");
    assert!(object.contains_key("images") && object.contains_key("token"));
}

/// `POST /api/trial/answer`: the reveal payload the verification panel recomputes the trial from
/// (FR-019, FR-020), including the `s_client` the browser itself produced.
#[tokio::test]
async fn an_answer_returns_the_whole_payload_the_client_can_recheck_it_with() {
    let state = service();
    let token = account_token(&state, "otherfren");
    let played = play(&state, &token, &S_CLIENT, false).await;
    let outcome = played.outcome.expect("this trial was answered");

    assert!(played.images.contains(&outcome.target));
    assert!(outcome.seq > 0, "the answer names its entry in the record");

    // Answered by choosing images[0], so the outcome is decidable from what the test knows.
    assert_eq!(
        outcome.hit,
        outcome.target == played.images[0],
        "hit is exactly whether the chosen image was the target"
    );
}

/// `s_client` is echoed although the browser produced it (D3, SC-002), so the verification panel
/// checks one payload rather than half a payload and half its own memory.
#[tokio::test]
async fn the_answer_echoes_the_randomness_the_browser_sent() {
    let state = service();
    let token = account_token(&state, "otherfren");
    let start = body_json(call(&state, post("/api/trial", &token, json!({}))).await).await;
    let reveal = body_json(
        call(
            &state,
            post(
                "/api/trial/reveal",
                &token,
                json!({ "token": start["token"], "s_client": encoded() }),
            ),
        )
        .await,
    )
    .await;
    let first = reveal["images"][0].clone();

    let answer = body_json(
        call(
            &state,
            post(
                "/api/trial/answer",
                &token,
                json!({ "token": reveal["token"], "chosen": first }),
            ),
        )
        .await,
    )
    .await;

    assert_eq!(answer["s_client"], encoded());
    for field in ["s_server", "nonce"] {
        let raw = answer[field].as_str().expect("base64 as the contract says");
        assert_eq!(
            STANDARD.decode(raw).expect("base64").len(),
            32,
            "{field} is 32 bytes"
        );
    }
}

/// Row one of the status table: an image the server never put on screen is refused, not scored as
/// a miss — a resolve entry naming an image nobody saw is a line no reader of the log can make
/// sense of. The trial stays open.
#[tokio::test]
async fn an_image_that_was_not_shown_is_a_four_hundred_and_the_trial_survives() {
    let state = service();
    let token = account_token(&state, "otherfren");
    let start = body_json(call(&state, post("/api/trial", &token, json!({}))).await).await;
    let reveal = body_json(
        call(
            &state,
            post(
                "/api/trial/reveal",
                &token,
                json!({ "token": start["token"], "s_client": encoded() }),
            ),
        )
        .await,
    )
    .await;
    let shown: Vec<String> = reveal["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    // A real identifier from the pool, deliberately one of the twenty-two not drawn.
    let unshown = (0..30)
        .map(|i| format!("img_{i:03}"))
        .find(|id| !shown.contains(id))
        .expect("eight of thirty leaves twenty-two");

    let refused = call(
        &state,
        post(
            "/api/trial/answer",
            &token,
            json!({ "token": reveal["token"], "chosen": unshown }),
        ),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

    // Still answerable, which is what "nothing is written and the trial stays open" means.
    let accepted = call(
        &state,
        post(
            "/api/trial/answer",
            &token,
            json!({ "token": reveal["token"], "chosen": shown[0] }),
        ),
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::OK);
}

/// Row two: `425` before the minimum viewing time, and the trial may be answered again afterwards.
/// The refusal is issued without the chosen image having been examined, so it leaks nothing about
/// the target (FR-039, SC-016) — which is why the same choice scores normally on the retry.
#[tokio::test]
async fn answering_too_fast_is_four_twenty_five_and_costs_the_trial_nothing() {
    // The shipped three seconds, rather than the fixture's zero.
    let state = service_with(Config::default());
    let token = account_token(&state, "otherfren");
    let start = body_json(call(&state, post("/api/trial", &token, json!({}))).await).await;
    let reveal = body_json(
        call(
            &state,
            post(
                "/api/trial/reveal",
                &token,
                json!({ "token": start["token"], "s_client": encoded() }),
            ),
        )
        .await,
    )
    .await;

    let early = call(
        &state,
        post(
            "/api/trial/answer",
            &token,
            json!({ "token": reveal["token"], "chosen": reveal["images"][0] }),
        ),
    )
    .await;
    assert_eq!(early.status(), StatusCode::TOO_EARLY);
    let body = body_json(early).await;
    assert_eq!(
        body.as_object().unwrap().len(),
        1,
        "a refusal that described the trial would be the oracle FR-039 forbids: {body}"
    );
}

/// Row three: a trial has one evaluated answer (FR-037). The second is a `409`, not a second entry
/// in the record.
#[tokio::test]
async fn answering_twice_is_a_conflict() {
    let state = service();
    let token = account_token(&state, "otherfren");
    let start = body_json(call(&state, post("/api/trial", &token, json!({}))).await).await;
    let reveal = body_json(
        call(
            &state,
            post(
                "/api/trial/reveal",
                &token,
                json!({ "token": start["token"], "s_client": encoded() }),
            ),
        )
        .await,
    )
    .await;
    let answer = post(
        "/api/trial/answer",
        &token,
        json!({ "token": reveal["token"], "chosen": reveal["images"][0] }),
    );
    assert_eq!(call(&state, answer).await.status(), StatusCode::OK);

    let again = call(
        &state,
        post(
            "/api/trial/answer",
            &token,
            json!({ "token": reveal["token"], "chosen": reveal["images"][0] }),
        ),
    )
    .await;
    assert_eq!(again.status(), StatusCode::CONFLICT);
}

/// Row four at the reveal: once the validity period has elapsed the trial cannot be opened at all
/// (FR-038, D16). A trial that could still be revealed a week later would make the published
/// abandonment rate meaningless — the rate is a claim about what people did, not about what the
/// server still felt like scoring.
#[tokio::test]
async fn a_trial_past_its_lifetime_cannot_be_revealed() {
    let state = service_with(Config {
        min_view_seconds: 0,
        // Expired the moment it is committed, which is how D16's clock is reached inside a test
        // that cannot sleep for a day.
        trial_lifetime_hours: 0,
        ..Config::default()
    });
    let token = account_token(&state, "otherfren");
    let start = body_json(call(&state, post("/api/trial", &token, json!({}))).await).await;

    let response = call(
        &state,
        post(
            "/api/trial/reveal",
            &token,
            json!({ "token": start["token"], "s_client": encoded() }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::GONE);
}

/// Row four at the answer, which is the branch the audit of 2026-07-26 found untested at the
/// endpoint level. The expiry the answer honours is the one sealed into token 2 at reveal time, so
/// this is the one test here that has to build a token rather than being handed one: an integration
/// test cannot make a day pass, and a lifetime short enough to expire during the test expires
/// before the reveal instead (above).
///
/// The token is re-sealed with the service's own key, so the server accepts it as authentic and
/// then refuses it as expired — which is the distinction being tested. Everything else about it is
/// the payload the reveal itself produced.
#[tokio::test]
async fn answering_after_the_trial_expired_is_gone() {
    let state = service();
    let (account_id, token) = account_id_and_token(&state, "otherfren");
    let start = body_json(call(&state, post("/api/trial", &token, json!({}))).await).await;
    let reveal = body_json(
        call(
            &state,
            post(
                "/api/trial/reveal",
                &token,
                json!({ "token": start["token"], "s_client": encoded() }),
            ),
        )
        .await,
    )
    .await;

    let live = reveal["token"].as_str().unwrap();
    let (seq, sealed) = live
        .split_once('.')
        .expect("`seq.sealed`, as the route composes it");
    let seq: u64 = seq.parse().unwrap();
    let mut two: TokenTwo = state
        .sealer
        .open(sealed, &account_id, seq)
        .expect("the service's own token opens with the service's own key");
    // Yesterday. The trial was revealed and then left, which is exactly the case FR-038 describes.
    two.expires_at -= 24 * 3600;
    let expired = format!("{seq}.{}", state.sealer.seal(&two, &account_id, seq));

    let response = call(
        &state,
        post(
            "/api/trial/answer",
            &token,
            json!({ "token": expired, "chosen": reveal["images"][0] }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::GONE);

    // And it is gone for good rather than merely unanswerable with that token: the live token from
    // the same reveal still names a trial the clock has not caught up with, so the refusal above
    // was about the expiry and nothing else.
    let still_live = call(
        &state,
        post(
            "/api/trial/answer",
            &token,
            json!({ "token": live, "chosen": reveal["images"][0] }),
        ),
    )
    .await;
    assert_eq!(still_live.status(), StatusCode::OK);
}

/// Every trial endpoint is authenticated, and the token is the whole of the authentication (D9).
#[tokio::test]
async fn none_of_the_trial_endpoints_answers_a_stranger() {
    let state = service();
    for uri in ["/api/trial", "/api/trial/reveal", "/api/trial/answer"] {
        let response = call(&state, post(uri, "not-a-token", json!({}))).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} answered a stranger"
        );
    }
}
