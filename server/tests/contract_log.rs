//! Conformance against `contracts/public-log.md`, section "What a verifier can check".
//!
//! Everything here is done the way a stranger does it: play trials through the HTTP API, download
//! the export, and then close the door on the server. From that point the only inputs are the
//! bytes of the file and the pool manifest served at `GET /api/pool/{version}/manifest` — no
//! database handle, no in-memory state, no figure the server was asked to report about itself.
//!
//! That restriction is the test. A verification suite that reaches into the process it is
//! verifying passes on a server that is lying to it.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use server::http::AppState;
use server::log::chain::{self, Body as EntryBody, Entry};
use server::log::export;
use server::pool::Manifest;
use server::trial::commit;
use server::trial::derive;

mod common;
use common::*;

/// Trials per account that are played to the end.
const ANSWERED: usize = 4;
/// Trials per account that are started, revealed and then walked away from. Below
/// `open_trials_per_account`, since an unresolved trial holds its slot.
const ABANDONED: usize = 2;

/// Three accounts, each answering four trials and walking away from two.
async fn played(state: &AppState) -> Vec<Played> {
    let mut all = Vec::new();
    for a in 0..3u8 {
        let token = account_token(state, &format!("otherfren{a}"));
        all.extend(play_many(state, &token, a, ANSWERED, ABANDONED).await);
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

    // The manifest was fetched from the same server that wrote the log, so on its own it proves
    // nothing: a version number is a pointer, and a pointer can be re-cut. Rehashing what was
    // served and holding it against the hash each commit entry published *before* the reveal is
    // what ties this recomputation to the pool the trial was actually sealed under (D34). Without
    // it every check below only establishes that the server agrees with itself.
    let served = Manifest::compute_hash(&manifest.categories, &manifest.images);
    for commit in &commits {
        let EntryBody::Commit {
            pool_manifest_hash, ..
        } = &commit.body
        else {
            unreachable!()
        };
        assert_eq!(
            pool_manifest_hash.as_deref(),
            Some(served.as_str()),
            "commit {} was sealed against a different pool than the one served",
            commit.seq
        );
    }

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
