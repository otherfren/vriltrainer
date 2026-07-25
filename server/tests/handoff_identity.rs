//! SC-007 and T090: a language switch preserves the account and creates no duplicate.
//!
//! Done the way the product does it — two processes, one per domain, sharing one database file
//! (D24). Two `Db` handles over one path is as close as a single test process gets to that, and it
//! is the arrangement the handoff depends on: the code minted on `.de` is redeemed on `.com` by a
//! lookup, with no traffic between the two.
//!
//! What is being denied here is the default behaviour. The domains are separate origins, so the
//! browser's copy of the access token does not travel; without a handoff the switch arrives as a
//! first-time visitor and the obliging thing to do is create an account. One person would then sit
//! in the leaderboard and in the aggregate twice, which is a wrong number in the figure the whole
//! site is an argument about (D11, FR-031).

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use tower::ServiceExt;

use server::config::{Config, Locale};
use server::db::Db;
use server::http::{AppState, router};
use server::pool::{ImageEntry, Manifest};
use server::trial::token::Sealer;

/// A database file removed with its WAL siblings when the test ends. By hand rather than with a
/// crate, matching `db::tests::TempDb`: the dependency would exist for six lines.
struct TempDb(PathBuf);

impl TempDb {
    fn new(tag: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!("vriltrainer-{tag}-{}.db", std::process::id()));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        TempDb(path)
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

/// Ten categories of three images — the smallest pool a trial can be drawn from.
fn pool() -> Manifest {
    let categories: Vec<String> = (0..10).map(|c| format!("cat{c}")).collect();
    let images: Vec<ImageEntry> = (0..30)
        .map(|i| ImageEntry {
            // Zero-padded, because the manifest is sorted ascending by id and that order *is* the
            // index the derivation draws against.
            id: format!("img_{i:03}"),
            category: format!("cat{}", i / 3),
        })
        .collect();
    let manifest = Manifest {
        version: 1,
        manifest_hash: Manifest::compute_hash(&categories, &images),
        categories,
        images,
    };
    manifest
        .validate()
        .expect("the fixture is a valid manifest");
    manifest
}

/// One domain's process: its own connection to the shared file, its own locale, and the same
/// sealing key — which is what `--token-key` gives the two units in the real deployment, and what
/// lets a trial started on one domain be finished on the other.
fn domain(path: &std::path::Path, locale: Locale) -> AppState {
    let config = Config {
        locale,
        // A test cannot wait out the real minimum viewing time, and this test is about identity
        // rather than about the timing gate, which has its own tests beside the handler.
        min_view_seconds: 0,
        ..Config::default()
    };
    AppState {
        db: Arc::new(Db::open(path).expect("the shared database opens")),
        config: Arc::new(config),
        sealer: Arc::new(Sealer::new(&[7u8; 32])),
        pool: Arc::new(pool()),
    }
}

async fn call(state: &AppState, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = router(state.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

fn post(uri: &str, token: Option<&str>, body: serde_json::Value) -> Request<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        // The per-address creation limit is one counter for the whole process, so this file uses
        // an address of its own.
        .header("x-forwarded-for", "203.0.113.90");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let mut request = request.body(Body::from(body.to_string())).unwrap();
    let peer: std::net::SocketAddr = "127.0.0.1:44321".parse().unwrap();
    request
        .extensions_mut()
        .insert(axum::extract::connect_info::ConnectInfo(peer));
    request
}

/// Plays one trial to the end and returns the sequence number of its resolve entry.
async fn play(state: &AppState, token: &str) -> u64 {
    let (status, start) = call(state, post("/api/trial", Some(token), json!({}))).await;
    assert_eq!(status, StatusCode::CREATED);

    let s_client = STANDARD.encode([9u8; 32]);
    let (status, reveal) = call(
        state,
        post(
            "/api/trial/reveal",
            Some(token),
            json!({ "token": start["token"], "s_client": s_client }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, answer) = call(
        state,
        post(
            "/api/trial/answer",
            Some(token),
            json!({ "token": reveal["token"], "chosen": reveal["images"][0] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    answer["seq"].as_u64().expect("the resolve entry is named")
}

/// Every account row in the shared database, as `(id, public_id)`.
fn accounts(db: &Db) -> Vec<(String, String)> {
    let reader = db.reader().unwrap();
    let mut stmt = reader
        .prepare("SELECT id, public_id FROM account ORDER BY created_at, id")
        .unwrap();
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    rows.map(Result::unwrap).collect()
}

/// The account every log entry is attributed to, in sequence order.
fn attributed(db: &Db) -> Vec<String> {
    let reader = db.reader().unwrap();
    let mut stmt = reader
        .prepare("SELECT account_id FROM log_entry ORDER BY seq")
        .unwrap();
    let rows = stmt.query_map([], |r| r.get(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

#[tokio::test]
async fn a_language_switch_keeps_the_account_and_creates_no_duplicate() {
    let file = TempDb::new("handoff");
    let de = domain(&file.0, Locale::De);
    let com = domain(&file.0, Locale::En);

    // Signing up on the German domain, and playing enough that losing the account would be
    // visible: the history is the thing SC-007 is about.
    let (status, created) = call(
        &de,
        post("/api/account", None, json!({ "name": "otherfren" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let token_de = created["access_token"].as_str().unwrap().to_string();
    let public_id = created["public_id"].as_str().unwrap().to_string();

    let first = play(&de, &token_de).await;
    let signed_up = accounts(&de.db);
    let [(account_id, _)] = signed_up.as_slice() else {
        panic!("one account, and one only");
    };
    let account_id = account_id.clone();

    // The switch: mint on the domain the user is leaving, redeem on the one they arrive at.
    let (status, minted) = call(&de, post("/api/handoff", Some(&token_de), json!({}))).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(minted["expires_in"], 30);
    let code = minted["code"].as_str().unwrap().to_string();

    let (status, redeemed) = call(
        &com,
        post("/api/handoff/redeem", None, json!({ "code": code })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token_com = redeemed["access_token"].as_str().unwrap().to_string();

    // SC-007, the negative half: nothing was created on arrival.
    assert_eq!(
        accounts(&com.db),
        vec![(account_id.clone(), public_id)],
        "the switch must not mint a second account"
    );

    // And the positive half: the same account, still holding the same history, playing on.
    let second = play(&com, &token_com).await;
    assert!(second > first);
    assert_eq!(
        attributed(&com.db),
        vec![account_id.clone(); 4],
        "every entry before and after the switch belongs to the one account"
    );
    assert_eq!(com.db.verify_chain().unwrap(), 4);

    // The code is spent. A second click on a stale link — the back button, a reload of the target
    // page — must open nothing.
    let (status, _) = call(
        &com,
        post("/api/handoff/redeem", None, json!({ "code": code })),
    )
    .await;
    assert_eq!(status, StatusCode::GONE);

    // The token the user arrives with is a new one, because the old one was never stored in a form
    // that could be handed back (D9). The German domain's copy is therefore dead, and its holder
    // is the same person, now on `.com`.
    assert_ne!(token_com, token_de);
    let (status, _) = call(&de, post("/api/trial", Some(&token_de), json!({}))).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "one account holds one live token"
    );

    // Switching back is the same move in the other direction, and still one account.
    let (_, minted) = call(&com, post("/api/handoff", Some(&token_com), json!({}))).await;
    let (status, redeemed) = call(
        &de,
        post(
            "/api/handoff/redeem",
            None,
            json!({ "code": minted["code"] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token_back = redeemed["access_token"].as_str().unwrap().to_string();
    play(&de, &token_back).await;

    assert_eq!(accounts(&de.db).len(), 1);
    assert_eq!(attributed(&de.db), vec![account_id; 6]);
    assert_eq!(de.db.verify_chain().unwrap(), 6);
}
