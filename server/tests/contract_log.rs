//! Conformance against `contracts/public-log.md`, section "What a verifier can check".
//!
//! Everything here is done the way a stranger does it: play trials through the HTTP API, download
//! the export, and then close the door on the server. From that point the only inputs are the
//! bytes of the file and the pool manifest served at `GET /api/pool/{version}/manifest` — no
//! database handle, no in-memory state, no figure the server was asked to report about itself.
//!
//! That restriction is the test. A verification suite that reaches into the process it is
//! verifying passes on a server that is lying to it.

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
use server::trial::commit;
use server::trial::derive;
use server::trial::token::Sealer;

/// Trials per account that are played to the end.
const ANSWERED: usize = 4;
/// Trials per account that are started, revealed and then walked away from. Below
/// `open_trials_per_account`, since an unresolved trial holds its slot.
const ABANDONED: usize = 2;

/// A service with a pool large enough to draw from: ten categories of three images.
fn service() -> AppState {
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

    let config = Config {
        // A test cannot wait out the real minimum viewing time, and this test is about the record
        // rather than about the timing gate — which has its own tests next to the handler.
        min_view_seconds: 0,
        ..Config::default()
    };

    AppState {
        db: Arc::new(Db::open_in_memory().expect("an in-memory database opens")),
        config: Arc::new(config),
        sealer: Arc::new(Sealer::new(&[11u8; 32])),
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

/// What the participant's own browser saw, kept so the export can be checked against it.
struct Played {
    trial_id: String,
    /// The eight identifiers as displayed, in order.
    images: Vec<String>,
    /// `None` for a trial that was abandoned after the reveal.
    outcome: Option<Outcome>,
}

struct Outcome {
    hit: bool,
    target: String,
    seq: u64,
}

/// Start, reveal, and answer unless `abandon`.
async fn play(state: &AppState, token: &str, s_client: &[u8; 32], abandon: bool) -> Played {
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

/// Three accounts, each answering four trials and walking away from two.
async fn played(state: &AppState) -> Vec<Played> {
    let mut all = Vec::new();
    for a in 0..3u8 {
        let token = account::create(&state.db, &format!("otherfren{a}"), &now_rfc3339())
            .expect("the fixture names pass the filter")
            .access_token;

        // Distinct client randomness per trial, so no two trials share a seed and a check that
        // passed by accident on a repeated draw would show up as a duplicate target.
        for t in 0..(ANSWERED + ABANDONED) {
            let mut s_client = [0u8; 32];
            s_client[0] = a;
            s_client[1] = t as u8;
            all.push(play(state, &token, &s_client, t >= ANSWERED).await);
        }
    }
    all
}

/// The export, as a third party gets it: over HTTP, paged, with no more than the endpoint offers.
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

/// The manifest as a verifier obtains it — over HTTP, by the version the entries name.
async fn manifest_for(state: &AppState, version: u32) -> Manifest {
    let request = Request::builder()
        .uri(format!("/api/pool/{version}/manifest"))
        .body(Body::empty())
        .unwrap();
    let response = call(state, request).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a trial recorded under v{version} is only verifiable while v{version}'s manifest answers"
    );
    serde_json::from_str(&body_text(response).await).unwrap()
}

fn decode(field: &str) -> Vec<u8> {
    STANDARD.decode(field).expect("base64 as the contract says")
}

/// Every check `contracts/public-log.md` names, run over the downloaded file and nothing else.
#[tokio::test]
async fn the_export_is_verifiable_by_someone_who_only_has_the_file() {
    let state = service();
    let trials = played(&state).await;
    let entries = download(&state).await;

    // ---- 1. The chain -----------------------------------------------------------------------
    // Including the gap check: a missing sequence number is as much evidence as an altered field,
    // and an operator dropping an inconvenient trial has to break one of the two.
    chain::verify(&entries).expect("the downloaded file is a chain");

    let head: serde_json::Value = {
        let request = Request::builder()
            .uri("/api/log/head")
            .body(Body::empty())
            .unwrap();
        body_json(call(&state, request).await).await
    };
    let last = entries.last().expect("trials were played");
    assert_eq!(head["seq"], last.seq);
    assert_eq!(
        head["entry_hash"], last.hash,
        "a head that does not match the last line stands behind nothing"
    );

    // ---- The two entry shapes, indexed by trial ----------------------------------------------
    let commits: Vec<&Entry> = entries
        .iter()
        .filter(|e| matches!(e.body, EntryBody::Commit { .. }))
        .collect();
    let resolves: Vec<&Entry> = entries
        .iter()
        .filter(|e| matches!(e.body, EntryBody::Resolve { .. }))
        .collect();
    assert_eq!(commits.len(), trials.len());
    assert_eq!(
        resolves.len(),
        trials.iter().filter(|t| t.outcome.is_some()).count()
    );

    // Self-chosen names never appear (FR-023, FR-036, D13) — that is what lets a name be erased
    // without invalidating a single entry.
    let raw = export::to_ndjson(&entries);
    for a in 0..3u8 {
        assert!(
            !raw.contains(&format!("otherfren{a}")),
            "a self-chosen name reached the hashed record"
        );
    }

    let pool_versions: Vec<u32> = commits
        .iter()
        .map(|e| match &e.body {
            EntryBody::Commit { pool_version, .. } => *pool_version,
            _ => unreachable!(),
        })
        .collect();
    let version = pool_versions[0];
    assert!(pool_versions.iter().all(|v| *v == version));
    let manifest = manifest_for(&state, version).await;
    let members = manifest.members();

    for resolve in &resolves {
        let EntryBody::Resolve {
            trial,
            chosen,
            target,
            hit,
            s_server,
            s_client,
            nonce,
        } = &resolve.body
        else {
            unreachable!()
        };

        let paired = commits
            .iter()
            .find(|c| c.body.trial() == trial)
            .expect("a resolve without its commit is a record nobody can read");
        let EntryBody::Commit {
            coordinate,
            commitment,
            ..
        } = &paired.body
        else {
            unreachable!()
        };

        // ---- 2. Each commitment --------------------------------------------------------------
        // The coordinate is inside the hash, so this also proves *this* coordinate belongs to
        // *this* trial rather than having been paired with it afterwards.
        assert!(
            commit::verify(&decode(s_server), &decode(nonce), coordinate, commitment),
            "trial {trial}: the target was not fixed before the choice"
        );

        // ---- 3. Each derivation --------------------------------------------------------------
        // Both contributions come out of the resolve entry. This is the check that stops working
        // if `s_client` is ever dropped from the log — the file would still verify as a chain and
        // still show hit rates, and only the participant could confirm any of it (T061, SC-002).
        let draw = derive::derive(&decode(s_server), &decode(s_client), &members)
            .expect("the published manifest can fill a trial");
        assert_eq!(
            manifest.images[draw.target_image()].id,
            *target,
            "trial {trial}: the published target is not the one the seed derives"
        );

        let shown: Vec<&str> = draw
            .images_in_display_order()
            .iter()
            .map(|i| manifest.images[*i].id.as_str())
            .collect();
        assert!(
            shown.contains(&chosen.as_str()),
            "trial {trial}: the chosen image was never on screen"
        );
        assert_eq!(*hit, chosen == target);

        // What the participant actually saw, against what the file says they saw.
        let played = trials.iter().find(|t| t.trial_id == *trial).unwrap();
        assert_eq!(shown, played.images, "trial {trial}: display order differs");
        let outcome = played.outcome.as_ref().unwrap();
        assert_eq!(outcome.target, *target);
        assert_eq!(outcome.hit, *hit);
        assert_eq!(outcome.seq, resolve.seq, "the answer named another entry");
    }

    // ---- 4. Abandonment ----------------------------------------------------------------------
    // Counted the way a stranger counts it: commits with no resolve. Nothing marks these entries,
    // which is the point — an operator cannot drop the inconvenient ones without breaking check 1.
    let resolved: Vec<&str> = resolves.iter().map(|e| e.body.trial()).collect();
    let mut abandoned: Vec<&str> = commits
        .iter()
        .map(|e| e.body.trial())
        .filter(|t| !resolved.contains(t))
        .collect();
    abandoned.sort_unstable();

    let mut expected: Vec<&str> = trials
        .iter()
        .filter(|t| t.outcome.is_none())
        .map(|t| t.trial_id.as_str())
        .collect();
    expected.sort_unstable();
    assert_eq!(abandoned, expected);

    // ---- 5. The aggregate --------------------------------------------------------------------
    let audit = export::audit(&entries);
    assert!(audit.covers_whole_log());
    assert_eq!(audit.commits, trials.len() as u64);
    assert_eq!(audit.resolves, resolves.len() as u64);
    assert_eq!(audit.abandoned, (3 * ABANDONED) as u64);
    assert_eq!(audit.accounts, 3);

    let hits = trials
        .iter()
        .filter_map(|t| t.outcome.as_ref())
        .filter(|o| o.hit)
        .count() as u64;
    assert_eq!(
        audit.hits, hits,
        "the headline figure must reproduce from the file alone (SC-004)"
    );
    assert_eq!(audit.hit_rate(), Some(hits as f64 / (3 * ANSWERED) as f64));
}

/// The single alteration an operator would most want to make: drop a trial that went badly.
///
/// It is worth asserting rather than assuming, because the export is the only copy a third party
/// has and "you would notice" is exactly the kind of claim that turns out to be false.
#[tokio::test]
async fn removing_an_inconvenient_trial_from_the_file_is_detectable() {
    let state = service();
    played(&state).await;
    let entries = download(&state).await;

    let victim = entries
        .iter()
        .position(|e| matches!(e.body, EntryBody::Resolve { .. }))
        .expect("some trial resolved");

    let mut doctored = entries.clone();
    doctored.remove(victim);
    assert!(
        chain::verify(&doctored).is_err(),
        "a deletion left the chain intact"
    );

    // And renumbering to close the gap does not help: `prev` is inside every hash.
    let mut renumbered = doctored;
    for (i, e) in renumbered.iter_mut().enumerate() {
        e.seq = i as u64 + 1;
    }
    assert!(chain::verify(&renumbered).is_err());
}

/// A file that starts partway into the log says so, instead of reporting a window as a rate.
#[tokio::test]
async fn a_partial_download_does_not_pass_itself_off_as_the_whole_record() {
    let state = service();
    played(&state).await;
    let entries = download(&state).await;

    let tail = &entries[entries.len() / 2..];
    let audit = export::audit(tail);
    assert!(
        !audit.covers_whole_log(),
        "resolves whose commit is out of range are the signal that this is a window"
    );
}
