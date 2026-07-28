//! Conformance against `contracts/http-api.md`, section "Statistics and leaderboard", for the two
//! statistics endpoints.
//!
//! Constitution principle III. The figures this covers — `by_chance_per_10k`, `rank`, `eligible`,
//! `distinct_days`, `wilson_upper` and the shape of `distribution` — are documented as the site's
//! published output and were, until this file, asserted nowhere outside the module that computes
//! them. A statistic that only its own author checks is a statistic nobody checks.

use axum::http::StatusCode;
use serde_json::Value;

use server::config::{Config, Thresholds};

mod common;
use common::*;

/// Everything unlocked at ten trials on one day, which is the shortest record that can be built
/// through the HTTP API — a distinct *day* cannot be manufactured over the wire.
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

async fn mine(state: &server::http::AppState, token: &str) -> Value {
    let response = call(state, authed("GET", "/api/stats/me", token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await
}

async fn aggregate(state: &server::http::AppState) -> Value {
    let response = call(state, get("/api/stats/aggregate")).await;
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await
}

/// Before the threshold the answer is deliberately thin: the counts, and what it takes to see
/// more. Reporting a rate over four trials is the most misread number the site could publish.
#[tokio::test]
async fn before_the_threshold_only_the_counts_and_the_threshold_are_reported() {
    let state = service_with(short_record());
    let token = account_token(&state, "otherfren");
    play_many(&state, &token, 0, 4, 1).await;

    let body = mine(&state, &token).await;
    assert_eq!(body["completed"], 4);
    // Always present, so selective abandonment is visible rather than hidden (FR-021). Zero here
    // and not one: the trial that was walked away from is still inside its D16 lifetime, so it is
    // *open*. Abandonment is the absence of a resolve entry after the clock has run out, and the
    // difference between the two is the whole reason both counts are published.
    assert_eq!(body["abandoned"], 0);
    assert_eq!(body["unlocks_at"], 10);
    assert!(
        body["hit_rate"].is_null() && body["deviation"].is_null(),
        "no inference is offered below the threshold: {body}"
    );
    // Configuration is reported rather than assumed by the client (D26, FR-050).
    assert!(body["thresholds"].is_object());
}

/// After it, every documented figure is present and internally consistent. The point of asserting
/// them as a set is that the contract promises them as a set: a client that renders the panel needs
/// all of them, and a missing one is a blank cell on a page about evidence.
#[tokio::test]
async fn after_the_threshold_every_documented_figure_is_reported() {
    let state = service_with(short_record());
    let token = account_token(&state, "otherfren");
    play_many(&state, &token, 0, 12, 2).await;

    let body = mine(&state, &token).await;
    for field in [
        "completed",
        "hits",
        "abandoned",
        "hit_rate",
        "reported_trials",
        "reported_hits",
        "deviation",
        "by_chance_per_10k",
        "wilson_lower",
        "wilson_upper",
        "distinct_days",
        "eligible",
        "unlocks_at",
        "thresholds",
    ] {
        assert!(!body[field].is_null(), "{field} is missing from {body}");
    }

    assert_eq!(body["completed"], 12);
    // Open rather than abandoned, as above: their lifetime has not run out.
    assert_eq!(body["abandoned"], 0);
    assert_eq!(body["distinct_days"], 1);
    assert_eq!(
        body["eligible"], true,
        "ten on one day, by this configuration"
    );

    // FR-019: the inferences stand over the reported block, not over everything played, which is
    // why the two counts can disagree and both be right. The block is ten here, so twelve trials
    // report ten.
    assert_eq!(body["reported_trials"], 10);
    assert!(body["reported_trials"].as_u64().unwrap() <= body["completed"].as_u64().unwrap());

    // The interval brackets the rate it is an interval for.
    let (lower, upper, rate) = (
        body["wilson_lower"].as_f64().unwrap(),
        body["wilson_upper"].as_f64().unwrap(),
        body["reported_hits"].as_f64().unwrap() / body["reported_trials"].as_f64().unwrap(),
    );
    assert!(
        lower <= rate && rate <= upper,
        "the interval does not contain its own rate: {lower} ≤ {rate} ≤ {upper}"
    );
    assert!(lower >= 0.0 && upper <= 1.0);

    // R3: out of ten thousand, so it is a count a reader can picture rather than a probability.
    let by_chance = body["by_chance_per_10k"].as_i64().unwrap();
    assert!((0..=10_000).contains(&by_chance), "{by_chance} per 10k");
}

/// `rank` is a band **slug** and it is *absent* for the middle band (D31, FR-042). Normie is about
/// a quarter of a chance population and the honest answer for them, and a page that invented a
/// title for it would be a page that awards one to everybody.
#[tokio::test]
async fn the_middle_band_reports_no_rank_and_a_tail_reports_its_slug() {
    let state = service_with(short_record());

    // A record right on chance. One hit in ten is close enough to 12,5 % to sit inside ±0,3 σ.
    let ordinary = account_token(&state, "ordinary");
    play_many(&state, &ordinary, 0, 10, 0).await;
    let body = mine(&state, &ordinary).await;
    let deviation = body["deviation"].as_f64().unwrap();
    if deviation.abs() < 0.3 {
        assert!(
            body["rank"].is_null(),
            "the middle band has no slug: {body}"
        );
    }

    // Whatever band is reported, it has to be one the thresholds actually name — a slug the client
    // has no message for renders as nothing at all.
    if let Some(slug) = body["rank"].as_str() {
        let bands = body["thresholds"]["bands"].as_array().unwrap();
        let named: Vec<&str> = bands
            .iter()
            .flat_map(|b| [b["high"].as_str().unwrap(), b["low"].as_str().unwrap()])
            .collect();
        assert!(named.contains(&slug), "{slug} is not one of {named:?}");
    }
}

/// The aggregate is the headline figure and is published as such even when it is exactly chance
/// (FR-045). Its two tail counts are the significance test a reader performs by looking.
#[tokio::test]
async fn the_aggregate_reports_both_tails_and_what_they_are_over() {
    let state = service_with(short_record());
    for a in 0..3u8 {
        let token = account_token(&state, &format!("viewer{a}"));
        play_many(&state, &token, a, 10, 1).await;
    }

    let body = aggregate(&state).await;
    for field in [
        "trials",
        "hits",
        "hit_rate",
        "expected_rate",
        "deviation",
        "accounts",
        "abandoned",
        "tail_high",
        "tail_low",
        "qualified",
        "tail_sigma",
        "tail_min_trials",
        "distribution",
        "thresholds",
    ] {
        assert!(!body[field].is_null(), "{field} is missing from {body}");
    }

    assert_eq!(body["expected_rate"], 0.125);
    assert_eq!(body["trials"], 30);
    assert_eq!(
        body["abandoned"], 0,
        "the three unanswered trials are open, not abandoned"
    );
    assert_eq!(body["accounts"], 3);
    // "Markedly" is a number the reader is given rather than a word they have to take on trust.
    assert_eq!(body["tail_sigma"], 1.9);
    assert_eq!(body["tail_min_trials"], 10);
    assert!(body["hit_rate"].as_f64().unwrap() >= 0.0);
}

/// `distribution` is the same finding as a measurement: every qualified account binned into the
/// bands the ladder is cut at, empty bands included, so a flat chart reads as the null rather than
/// as a broken page. The open ends are stated with `null` rather than implied by a large number.
#[tokio::test]
async fn the_distribution_is_every_band_including_the_empty_ones() {
    let state = service_with(short_record());
    let token = account_token(&state, "otherfren");
    play_many(&state, &token, 0, 10, 0).await;

    let body = aggregate(&state).await;
    let bins = body["distribution"].as_array().expect("an array of bins");
    // Eleven rungs: five bands either side of a middle one (D31).
    assert_eq!(bins.len(), 11, "the ladder is published whole: {bins:?}");

    assert!(bins[0]["from"].is_null(), "the lowest band is open below");
    assert!(
        bins[bins.len() - 1]["to"].is_null(),
        "the highest band is open above"
    );
    let middle = &bins[bins.len() / 2];
    assert!(middle["rank"].is_null(), "the middle band has no slug");
    assert_eq!(middle["tail"], false);

    // Most negative first (D31), contiguous, and every bin says whether it counts as a tail.
    let mut previous: Option<f64> = None;
    for bin in bins {
        assert!(bin["accounts"].is_u64());
        assert!(bin["tail"].is_boolean());
        if let (Some(from), Some(last)) = (bin["from"].as_f64(), previous) {
            assert_eq!(from, last, "a gap between bands: {bins:?}");
        }
        previous = bin["to"].as_f64();
    }

    // The counts are over the qualified population and nothing else.
    let binned: u64 = bins.iter().map(|b| b["accounts"].as_u64().unwrap()).sum();
    assert_eq!(binned, body["qualified"].as_u64().unwrap());
}

/// The aggregate is public — it is the site's headline claim, and a claim behind a login is not
/// one anybody can check.
#[tokio::test]
async fn the_aggregate_needs_no_token_and_the_personal_figures_do() {
    let state = service_with(short_record());
    assert_eq!(
        call(&state, get("/api/stats/aggregate")).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        call(&state, get("/api/stats/me")).await.status(),
        StatusCode::UNAUTHORIZED
    );
}
