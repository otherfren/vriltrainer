//! `POST /api/trial`, `/reveal` and `/answer` — the loop the product is.
//!
//! Three requests, and the order of the writes inside them is the product's central claim.
//!
//! - **Start** draws `s_server`, a nonce and a coordinate, and appends the `COMMIT` entry *before*
//!   the response is built (FR-007, FR-013, D3). A crash between the two loses a trial; the other
//!   order loses the proof that it was sealed, which is the one thing this service sells.
//! - **Reveal** takes the client's randomness and fixes the target — after both contributions
//!   exist and before any choice (D1, D3). The response is the same shape whatever the target is.
//! - **Answer** decides the timing question before it looks at the choice, appends `RESOLVE`, and
//!   only then hands back the randomness the client needs to recompute everything (FR-010,
//!   FR-022).
//!
//! Nothing about the target reaches the database in between. `s_server` lives in the sealed token
//! the client carries (D16), so a stolen backup contains no pending answers.

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::post;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use rand::{Rng, RngCore};
use rusqlite::{ErrorCode, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::db::{Db, DbError, now_rfc3339};
use crate::http::routes::account::Holder;
use crate::http::{ApiError, AppState};
use crate::log::chain::Body;
use crate::pool::Manifest;
use crate::stats::accumulate;
use crate::trial::commit::commitment;
use crate::trial::derive;
use crate::trial::timing::{self, Timing, now_unix};
use crate::trial::token::{TokenError, TokenOne, TokenTwo};

/// Bytes each side contributes to the seed. Fixed rather than "whatever arrives", so a client
/// cannot weaken its own half and then dispute the derivation afterwards.
const SEED_BYTES: usize = 32;

/// Bytes of trial identifier. It appears in every published entry for the trial, so it is drawn
/// rather than counted — a sequential one would let a reader of the export order trials by account
/// and count them per account without the accounts being named.
const TRIAL_ID_BYTES: usize = 16;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/trial", post(start))
        .route("/api/trial/reveal", post(reveal))
        .route("/api/trial/answer", post(answer))
}

#[derive(Serialize)]
struct StartResponse {
    trial_id: String,
    coordinate: String,
    /// `framed(s_server, nonce, coordinate)`. It travels **with** the coordinate: a proof produced
    /// only at the reveal verifies a claim the server was free to invent after seeing the pick,
    /// and is worthless (D3).
    commitment: String,
    pool_version: u32,
    /// Without this, nobody can recompute the draw — the manifest is half the derivation (D3, D5).
    pool_manifest_hash: String,
    token: String,
}

/// Starts a trial.
async fn start(
    State(state): State<AppState>,
    Holder(account): Holder,
) -> Result<Response, ApiError> {
    let s_server = random_bytes(SEED_BYTES);
    let nonce = random_bytes(SEED_BYTES);
    let coordinate = coordinate();
    let trial_id = random_hex(TRIAL_ID_BYTES);
    let commitment = commitment(&s_server, &nonce, &coordinate);

    let at = now_rfc3339();

    // No cap on how many trials an account may hold open at once. The operator removed it
    // deliberately; a trial left unanswered still expires on the D16 clock and is still published
    // as abandoned (FR-021), so the record stays honest without a gate in front of it.
    let entry = state.db.append(
        &at,
        Body::Commit {
            trial: trial_id.clone(),
            account: account.clone(),
            coordinate: coordinate.clone(),
            commitment: commitment.clone(),
            pool_version: state.pool.version,
            // The manifest itself, not a pointer to it. Half the derivation is which images sit at
            // which index, and a reader who cannot pin that down cannot recompute anything (D34).
            pool_manifest_hash: Some(state.pool.manifest_hash.clone()),
        },
    );

    let entry = match entry {
        Ok(committed) => committed,
        Err(e) => return Err(e.into()),
    };

    // Only now, with the entry on disk. The token is sealed against the sequence number the
    // append handed back, so it cannot even be minted before the record exists.
    let token = state.sealer.seal(
        &TokenOne {
            s_server,
            nonce,
            coordinate: coordinate.clone(),
            pool_version: state.pool.version,
            pool_manifest_hash: state.pool.manifest_hash.clone(),
        },
        &account,
        entry.seq,
    );

    Ok((
        StatusCode::CREATED,
        Json(StartResponse {
            trial_id,
            coordinate,
            commitment,
            pool_version: state.pool.version,
            pool_manifest_hash: state.pool.manifest_hash.clone(),
            token: compose(entry.seq, &token),
        }),
    )
        .into_response())
}

#[derive(Deserialize)]
struct RevealRequest {
    token: String,
    /// 32 bytes from `crypto.getRandomValues`, base64.
    s_client: String,
}

#[derive(Serialize)]
struct RevealResponse {
    /// Exactly eight identifiers, in derived display order.
    ///
    /// **Nothing else may join this struct.** The response is the same shape whatever the target
    /// is, and any additional per-image field — a category, a size, an order hint — is a channel
    /// distinguishing the target from its decoys (SC-011, FR-011).
    images: Vec<String>,
    token: String,
}

/// Supplies the client's randomness and returns the candidate set.
async fn reveal(
    State(state): State<AppState>,
    Holder(account): Holder,
    Json(request): Json<RevealRequest>,
) -> Result<Response, ApiError> {
    let (seq, sealed) = decompose(&request.token).ok_or(ApiError::BadRequest("malformed token"))?;
    let one: TokenOne = state.sealer.open(sealed, &account, seq).map_err(unsealed)?;

    let s_client = base64_bytes(&request.s_client)
        .filter(|b| b.len() == SEED_BYTES)
        .ok_or(ApiError::BadRequest(
            "s_client must be 32 base64-encoded bytes",
        ))?;

    same_pool(&state.pool, one.pool_version, &one.pool_manifest_hash)?;

    let commit = committed(&state.db, seq, &account)?.ok_or(ApiError::Gone)?;
    if commit.resolved {
        return Err(ApiError::AlreadyAnswered);
    }
    let expires_at = expiry(&commit.at, state.config.trial_lifetime_hours).ok_or(ApiError::Gone)?;
    let revealed_at = now_unix();
    if revealed_at >= expires_at {
        return Err(ApiError::Gone);
    }

    // The whole set comes out of one stream — target, decoys and order together (D22). Assembling
    // the decoys after the target were known is what lets a set be stacked so the target stands
    // out by resolution or subject matter, which is the classic way a forced-choice experiment
    // leaks.
    let draw = derive::derive(&one.s_server, &s_client, &state.pool.members()).map_err(|e| {
        tracing::error!(error = %e, "the published pool cannot fill a trial");
        ApiError::Internal
    })?;

    let images = draw
        .images_in_display_order()
        .iter()
        .map(|i| image_id(&state.pool, *i).map(str::to_string))
        .collect::<Option<Vec<String>>>()
        .ok_or(ApiError::Internal)?;

    let token = state.sealer.seal(
        &TokenTwo {
            s_server: one.s_server,
            s_client,
            nonce: one.nonce,
            coordinate: one.coordinate,
            pool_version: one.pool_version,
            pool_manifest_hash: one.pool_manifest_hash,
            selected: draw.selected_images.to_vec(),
            target_slot: draw.target_slot,
            display_order: draw.display_order.to_vec(),
            revealed_at,
            expires_at,
        },
        &account,
        seq,
    );

    Ok(Json(RevealResponse {
        images,
        token: compose(seq, &token),
    })
    .into_response())
}

#[derive(Deserialize)]
struct AnswerRequest {
    token: String,
    chosen: String,
}

#[derive(Serialize)]
struct AnswerResponse {
    hit: bool,
    target: String,
    /// The three secrets, released together and not a moment earlier. With these and the manifest
    /// the client recomputes the commitment and the whole draw (FR-019, FR-020, FR-022).
    ///
    /// `s_client` is echoed although the browser produced it: the panel that verifies the trial
    /// should be checking one payload rather than half a payload and half its own memory, and the
    /// published log entry carries both for the same reason (D3, SC-002).
    s_server: String,
    s_client: String,
    nonce: String,
    /// Where this trial's `RESOLVE` entry sits in the public record, so a user can point at it.
    seq: u64,
}

/// Scores the answer.
async fn answer(
    State(state): State<AppState>,
    Holder(account): Holder,
    Json(request): Json<AnswerRequest>,
) -> Result<Response, ApiError> {
    let (seq, sealed) = decompose(&request.token).ok_or(ApiError::BadRequest("malformed token"))?;
    let two: TokenTwo = state.sealer.open(sealed, &account, seq).map_err(unsealed)?;

    // ---- Nothing above this line has looked at `request.chosen`, and nothing may. ------------
    // The minimum viewing time is decided from the clock alone (FR-039, SC-016). Checked after
    // the choice were examined, the refusal itself would answer "was that the target?" for anyone
    // willing to guess in under three seconds and read the status code.
    match timing::gate(&two, now_unix(), state.config.min_view_seconds) {
        // Told it expired and offered a fresh trial, never silently scored as a miss (FR-038).
        Timing::Expired => return Err(ApiError::Gone),
        // Nothing is written: a speed-rejected answer does not consume the trial (FR-037).
        Timing::TooFast => return Err(ApiError::TooFast),
        Timing::Evaluate => {}
    }
    // -----------------------------------------------------------------------------------------

    same_pool(&state.pool, two.pool_version, &two.pool_manifest_hash)?;

    let commit = committed(&state.db, seq, &account)?.ok_or(ApiError::Gone)?;
    if commit.resolved {
        return Err(ApiError::AlreadyAnswered);
    }

    // In *selection* order, which is the order `target_slot` counts in — the display order is a
    // permutation applied on top of it, and reading the slot out of the shuffled list would score
    // a different image than the one the derivation named (D22).
    let shown: Vec<&str> = two
        .selected
        .iter()
        .map(|i| image_id(&state.pool, *i))
        .collect::<Option<Vec<&str>>>()
        .ok_or(ApiError::Internal)?;
    let target = shown
        .get(two.target_slot)
        .ok_or(ApiError::Internal)?
        .to_string();

    if !shown.contains(&request.chosen.as_str()) {
        // Refused rather than scored as a miss. A resolve entry naming an image that was never on
        // screen is a line in the public record that no reader can make sense of, and the
        // abandonment and hit figures are computed from that file by strangers.
        return Err(ApiError::BadRequest(
            "chosen was not one of the images shown",
        ));
    }
    let hit = request.chosen == target;

    // `append_with`, so the account's running figures move inside the same transaction as the
    // entry that justifies them. Counted afterwards in a second write, a crash in between leaves a
    // resolve in the log that no total includes, and the statistics page and the export — which
    // readers are invited to compare (SC-004, SC-012) — disagree with no way to tell which is
    // right. The cache can always be replayed from the log; the log can never be replayed from it.
    let (entry, ()) = state
        .db
        .append_with(
            &now_rfc3339(),
            Body::Resolve {
                trial: commit.trial,
                chosen: request.chosen,
                target: target.clone(),
                hit,
                s_server: base64(&two.s_server),
                s_client: base64(&two.s_client),
                nonce: base64(&two.nonce),
            },
            |tx, entry| accumulate::on_resolve(tx, &state.config, &account, hit, &entry.at),
        )
        .map_err(answered_concurrently)?;

    // After the append, never before. The randomness is the proof, and handing it over for a
    // trial whose resolve failed to commit would publish an outcome the record does not contain.
    Ok(Json(AnswerResponse {
        hit,
        target,
        s_server: base64(&two.s_server),
        s_client: base64(&two.s_client),
        nonce: base64(&two.nonce),
        seq: entry.seq,
    })
    .into_response())
}

/// What the log already knows about a trial, found through the sequence number its token is bound
/// to.
struct Committed {
    trial: String,
    /// The commit timestamp, which is the trial's clock (D16).
    at: String,
    /// Whether an evaluated answer already exists (FR-037).
    resolved: bool,
}

/// The commit entry for `seq`, if it belongs to `account`.
///
/// The account is in the `WHERE` clause although the token's own authentication already binds it
/// (D16): the sealing key is one secret, and a check that needs no second thought is worth the
/// clause it costs.
fn committed(db: &Db, seq: u64, account: &str) -> Result<Option<Committed>, DbError> {
    let reader = db.reader()?;
    let found = reader
        .query_row(
            "SELECT c.trial_id, c.at,
                    EXISTS (SELECT 1 FROM log_entry r
                             WHERE r.trial_id = c.trial_id AND r.kind = 'resolve')
               FROM log_entry c
              WHERE c.seq = ?1 AND c.kind = 'commit' AND c.account_id = ?2",
            params![seq, account],
            |r| {
                Ok(Committed {
                    trial: r.get(0)?,
                    at: r.get(1)?,
                    resolved: r.get::<_, i64>(2)? != 0,
                })
            },
        )
        .optional()?;
    Ok(found)
}

/// A `UNIQUE` violation on the resolve insert.
///
/// `log_entry_trial_kind` is the real defence against a second evaluated answer; the read above is
/// only what turns the common case into a clean 409. Two answers submitted at once both pass that
/// read, and the loser lands here (FR-037, D16). No other constraint on this insert can fire: the
/// chain's `UNIQUE`s cannot collide under `BEGIN IMMEDIATE`, and the account is copied from the
/// commit row rather than supplied.
fn answered_concurrently(e: DbError) -> ApiError {
    match &e {
        DbError::Sqlite(rusqlite::Error::SqliteFailure(f, _))
            if f.code == ErrorCode::ConstraintViolation =>
        {
            ApiError::AlreadyAnswered
        }
        _ => e.into(),
    }
}

/// Refuses a trial drawn under a manifest this process no longer serves.
///
/// One process holds one manifest, and the derivation is meaningless against another (D5, D22).
/// `Gone` rather than an error, because from the user's side that is what happened: the trial
/// cannot be completed and they should be given a new one (FR-038).
///
/// The hash is checked as well as the number, because the number can be re-cut (D34). A trial that
/// began under one v1 and finished under another would be scored against images its own commit
/// entry does not describe — an honest trial that fails verification, which is worse than a
/// refused one. `sealed_against` is empty only in a token minted before the field existed; those
/// are held to the version alone, as they were when they were issued.
fn same_pool(pool: &Manifest, drawn_under: u32, sealed_against: &str) -> Result<(), ApiError> {
    if pool.version == drawn_under
        && (sealed_against.is_empty() || sealed_against == pool.manifest_hash)
    {
        return Ok(());
    }
    tracing::warn!(
        serving = pool.version,
        serving_hash = %pool.manifest_hash,
        drawn_under,
        sealed_against,
        "a trial from an earlier pool cannot be completed by this process"
    );
    Err(ApiError::Gone)
}

/// A token that will not open.
///
/// `Gone` rather than `Unauthorized`, because the overwhelmingly likely cause is a restart without
/// `--token-key`: the sealing key was fresh, and every trial in flight became undecryptable. D16
/// requires that case to be explained as an expired trial with a new one offered, never scored as
/// a loss — and a forged token learns nothing from being told the same thing.
fn unsealed(e: TokenError) -> ApiError {
    match e {
        TokenError::Malformed => ApiError::BadRequest("malformed token"),
        TokenError::NotAuthentic | TokenError::Expired => ApiError::Gone,
    }
}

/// The wire form of a trial token: the log sequence number it is sealed against, a dot, then the
/// sealed payload.
///
/// The sequence number travels in clear because it is an *input* to opening the token — the seal
/// binds (account, sequence) as additional authenticated data (D16), so the server cannot decrypt
/// one without being told which trial it claims to be. That costs nothing: every commit sequence
/// number is published in the log anyway (FR-025). And it is authenticated, so editing it does not
/// open a different trial's token, it opens nothing.
fn compose(seq: u64, sealed: &str) -> String {
    format!("{seq}.{sealed}")
}

fn decompose(token: &str) -> Option<(u64, &str)> {
    let (seq, sealed) = token.split_once('.')?;
    Some((seq.parse().ok()?, sealed))
}

fn image_id(pool: &Manifest, index: usize) -> Option<&str> {
    pool.images.get(index).map(|e| e.id.as_str())
}

/// Padded standard base64, which is what `atob` reads and what `contracts/public-log.md` shows.
fn base64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Base64 in any of the spellings a browser helper might produce.
///
/// The contract says base64 and the client will send the padded standard alphabet. Refusing an
/// unpadded or URL-safe variant would cost a trial over an encoding detail that changes nothing
/// about the bytes, and the length is checked by the caller regardless.
fn base64_bytes(text: &str) -> Option<Vec<u8>> {
    STANDARD
        .decode(text)
        .or_else(|_| STANDARD_NO_PAD.decode(text))
        .or_else(|_| URL_SAFE_NO_PAD.decode(text))
        .ok()
}

/// When a trial committed at `at` stops being answerable (D16).
///
/// An unparsable timestamp reads as expired. This process writes the column with `now_rfc3339`, so
/// a value that will not parse means the row was edited by hand — and a hand-edited audit log
/// should cost a trial, not crash the service that would otherwise keep appending to it.
fn expiry(at: &str, lifetime_hours: i64) -> Option<i64> {
    Some(OffsetDateTime::parse(at, &Rfc3339).ok()?.unix_timestamp() + lifetime_hours * 3600)
}

/// The coordinate: `NNNN-NNNN`, uniform, encoding nothing (R6).
///
/// Drawn independently of `s_server`, and note that it is *not* derived from it: the commitment
/// binds the two together, and a coordinate computed from the seed would be one more thing a
/// reader has to be told is not a hint.
fn coordinate() -> String {
    let mut rng = rand::rng();
    format!(
        "{:04}-{:04}",
        rng.random_range(0..10_000u32),
        rng.random_range(0..10_000u32)
    )
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    rand::rng().fill_bytes(&mut b);
    b
}

fn random_hex(bytes: usize) -> String {
    hex::encode(random_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body as AxumBody;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;
    use crate::account;
    use crate::config::Config;
    use crate::http::router;
    use crate::pool::ImageEntry;
    use crate::trial::token::Sealer;

    /// A pool that can actually fill a trial: ten categories of three images. `test_support::state`
    /// deliberately ships a two-image manifest, which no draw can use.
    fn state_with_pool() -> AppState {
        let categories: Vec<String> = (0..10).map(|c| format!("cat{c}")).collect();
        let images: Vec<ImageEntry> = (0..30)
            .map(|i| ImageEntry {
                // Zero-padded, so the manifest is sorted ascending by id as `validate` requires.
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

        AppState {
            db: Arc::new(Db::open_in_memory().expect("an in-memory database opens")),
            config: Arc::new(Config::default()),
            sealer: Arc::new(Sealer::new(&[7u8; 32])),
            pool: Arc::new(manifest),
        }
    }

    /// The same service — same database, same sealing key, same trials — with a different minimum
    /// viewing time. A test cannot move the wall clock, so this stands in for waiting.
    fn with_min_view(state: &AppState, seconds: i64) -> AppState {
        let mut config = (*state.config).clone();
        config.min_view_seconds = seconds;
        AppState {
            config: Arc::new(config),
            ..state.clone()
        }
    }

    fn holder(state: &AppState) -> String {
        account::create(&state.db, "otherfren", &now_rfc3339())
            .expect("the fixture name passes the filter")
            .access_token
    }

    fn post(uri: &str, token: &str, body: serde_json::Value) -> Request<AxumBody> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(AxumBody::from(body.to_string()))
            .unwrap()
    }

    async fn json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// The refusal as the wire carries it. Comparing parsed JSON would hide a difference in
    /// length or key order, and a difference in length is a difference a stopwatch-free attacker
    /// can read.
    async fn status_and_bytes(response: Response) -> (StatusCode, Vec<u8>) {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        (status, bytes.to_vec())
    }

    async fn call(state: &AppState, request: Request<AxumBody>) -> Response {
        router(state.clone()).oneshot(request).await.unwrap()
    }

    /// Start, reveal, and the token to answer with, plus the eight images as shown.
    async fn revealed(state: &AppState, token: &str) -> (String, Vec<String>) {
        let start = call(state, post("/api/trial", token, serde_json::json!({}))).await;
        assert_eq!(start.status(), StatusCode::CREATED);
        let start = json(start).await;

        let s_client = base64(&[9u8; SEED_BYTES]);
        let reveal = call(
            state,
            post(
                "/api/trial/reveal",
                token,
                serde_json::json!({ "token": start["token"], "s_client": s_client }),
            ),
        )
        .await;
        assert_eq!(reveal.status(), StatusCode::OK);
        let reveal = json(reveal).await;

        let images = reveal["images"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        (reveal["token"].as_str().unwrap().to_string(), images)
    }

    fn resolves(state: &AppState) -> u32 {
        let reader = state.db.reader().unwrap();
        reader
            .query_row(
                "SELECT COUNT(*) FROM log_entry WHERE kind = 'resolve'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// FR-013 and D3: the trial is in the public record before the client learns the coordinate.
    ///
    /// The strong form of this is structural — the token is sealed against the sequence number the
    /// append hands back, so the response cannot be built before the entry exists. What the test
    /// can observe is the weaker but still load-bearing half: at the first instant the response is
    /// in the caller's hands, the entry is already there, and it holds the commitment the caller
    /// was given.
    #[tokio::test]
    async fn the_commit_entry_is_in_the_log_before_the_response_is_read() {
        let state = state_with_pool();
        let token = holder(&state);

        let response = call(&state, post("/api/trial", &token, serde_json::json!({}))).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        // The log is read before the body is even parsed.
        let (seq, _) = state.db.head().unwrap();
        assert_eq!(seq, 1, "the commit was not written before the response");
        let entry = state.db.entries_from(1, 1).unwrap().remove(0);

        let body = json(response).await;
        match entry.body {
            Body::Commit {
                trial,
                coordinate,
                commitment,
                pool_version,
                ..
            } => {
                assert_eq!(body["trial_id"], trial);
                assert_eq!(body["coordinate"], coordinate);
                assert_eq!(
                    body["commitment"], commitment,
                    "the published commitment must be the one the client was handed"
                );
                assert_eq!(body["pool_version"], pool_version);
            }
            other => panic!("expected a commit entry, got {other:?}"),
        }
        assert_eq!(body["pool_manifest_hash"], state.pool.manifest_hash);
        assert_eq!(state.db.verify_chain().unwrap(), 1);
    }

    /// SC-011. The reveal is the last response before a choice is made, so it is the one that must
    /// not distinguish the target — including by carrying a field nobody asked for.
    #[tokio::test]
    async fn the_reveal_returns_eight_images_and_nothing_that_names_the_target() {
        let state = state_with_pool();
        let token = holder(&state);
        let start = call(&state, post("/api/trial", &token, serde_json::json!({}))).await;
        let start = json(start).await;

        let response = call(
            &state,
            post(
                "/api/trial/reveal",
                &token,
                serde_json::json!({ "token": start["token"], "s_client": base64(&[1u8; SEED_BYTES]) }),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;

        let mut keys: Vec<&str> = body
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["images", "token"],
            "the reveal carries the candidates and the sealed state, and nothing else"
        );

        let images = body["images"].as_array().unwrap();
        assert_eq!(images.len(), derive::SET_SIZE);
        let mut ids: Vec<&str> = images.iter().map(|v| v.as_str().unwrap()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            derive::SET_SIZE,
            "the eight candidates must be distinct"
        );
        for id in ids {
            assert!(
                state.pool.images.iter().any(|e| e.id == id),
                "{id} is not in the pool"
            );
        }
    }

    /// FR-039 and SC-016, both halves. The refusal says nothing about the images, and the trial is
    /// still there afterwards — a rule that consumed the trial would be a rule nobody could use
    /// twice, and one that leaked would be an oracle for the target.
    #[tokio::test]
    async fn an_early_answer_is_refused_without_leaking_and_without_consuming_the_trial() {
        let state = state_with_pool();
        assert!(
            state.config.min_view_seconds > 0,
            "the shipped minimum is three seconds"
        );
        let token = holder(&state);
        let (trial_token, images) = revealed(&state, &token).await;

        let response = call(
            &state,
            post(
                "/api/trial/answer",
                &token,
                serde_json::json!({ "token": trial_token, "chosen": images[0] }),
            ),
        )
        .await;
        assert_eq!(response.status().as_u16(), 425);

        let body = json(response).await;
        assert_eq!(body, serde_json::json!({ "error": "too fast" }));
        let printed = body.to_string();
        for field in ["target", "s_server", "s_client", "nonce", "hit"] {
            assert!(!printed.contains(field), "the refusal mentions {field}");
        }
        for id in &images {
            assert!(!printed.contains(id), "the refusal names an image");
        }
        assert_eq!(resolves(&state), 0, "nothing may be written");

        // The same trial, answered once the minimum has passed. The token, the account and the
        // database are the ones from above; only the clock is different.
        let patient = with_min_view(&state, 0);
        let response = call(
            &patient,
            post(
                "/api/trial/answer",
                &token,
                serde_json::json!({ "token": trial_token, "chosen": images[0] }),
            ),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "a speed-rejected answer must not consume the trial"
        );
        let body = json(response).await;
        assert!(body["hit"].is_boolean());
        assert!(images.contains(&body["target"].as_str().unwrap().to_string()));
        assert_eq!(resolves(&state), 1);
        assert_eq!(state.db.verify_chain().unwrap(), 2);
    }

    /// FR-039 and SC-016, in the form an attacker would use. The test above submits one image;
    /// this submits all eight plus a name that was never on screen, and requires the nine refusals
    /// to be indistinguishable byte for byte. If any of them differed — a status code, a word, a
    /// length — a viewer could read the target off eight guesses made inside three seconds and
    /// never spend the trial, because none of them writes anything.
    ///
    /// The image that is not in the pool is in the list on purpose. The membership check lives
    /// *after* the gate, so a `400` here would be proof that the gate had already looked at the
    /// choice.
    #[tokio::test]
    async fn the_speed_refusal_is_identical_for_every_candidate() {
        let state = state_with_pool();
        assert!(state.config.min_view_seconds > 0);
        let token = holder(&state);
        let (trial_token, images) = revealed(&state, &token).await;
        let head_before = state.db.head().unwrap();

        let mut probes = images.clone();
        probes.push("img_999".to_string());
        assert!(!state.pool.images.iter().any(|e| e.id == "img_999"));

        let mut refusals = Vec::new();
        for chosen in &probes {
            let response = call(
                &state,
                post(
                    "/api/trial/answer",
                    &token,
                    serde_json::json!({ "token": trial_token, "chosen": chosen }),
                ),
            )
            .await;
            refusals.push(status_and_bytes(response).await);
        }

        assert_eq!(refusals[0].0.as_u16(), 425);
        for (i, refusal) in refusals.iter().enumerate() {
            assert_eq!(
                refusal, &refusals[0],
                "the refusal for {} can be told apart from the others",
                probes[i]
            );
        }

        assert_eq!(
            state.db.head().unwrap(),
            head_before,
            "nine guesses inside the minimum must leave the record exactly as it was"
        );
        assert_eq!(resolves(&state), 0);

        // And the trial is still there to be answered honestly.
        let patient = with_min_view(&state, 0);
        let response = call(
            &patient,
            post(
                "/api/trial/answer",
                &token,
                serde_json::json!({ "token": trial_token, "chosen": images[0] }),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// SC-011 as a measurement rather than a field list. The reveal sends
    /// [`Draw::images_in_display_order`] and the answer scores
    /// `selected_images[target_slot]` — the seam where an off-by-one or a forgotten permutation
    /// would show up. Three strategies a client could actually run against the visible list must
    /// all score chance:
    ///
    /// - take the image at display position *p*, for every *p*;
    /// - take the one with the lowest manifest index, which is what sorting the identifiers gives;
    /// - take the one drawn from the largest category, the bias D22's step 3 exists to remove.
    ///
    /// Deterministic seeds, so a failure is reproducible rather than a flake. The pool is
    /// deliberately lopsided — one category holding 300 of 355 images — and its indices are
    /// interleaved across categories, so "lowest index" is a real strategy and not an artefact of
    /// categories occupying contiguous blocks.
    #[test]
    fn nothing_visible_in_the_reveal_predicts_the_target() {
        const ROUNDS: usize = 60_000;
        const BIG: usize = 0;

        let sizes = [300usize, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5];
        let total: usize = sizes.iter().sum();
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); sizes.len()];
        let mut left = sizes;
        let mut index = 0usize;
        while index < total {
            for (c, remaining) in left.iter_mut().enumerate() {
                if *remaining > 0 {
                    members[c].push(index);
                    *remaining -= 1;
                    index += 1;
                }
            }
        }

        let mut by_position = [0u32; derive::SET_SIZE];
        let mut lowest_index = 0u32;
        let mut big_shown = 0u32;
        let mut big_was_target = 0u32;

        for round in 0..ROUNDS {
            let draw = derive::derive(&round.to_le_bytes(), b"fixed-client", &members).unwrap();
            let shown = draw.images_in_display_order();
            let target = draw.selected_images[draw.target_slot];

            // The structural half: a display order that dropped or duplicated a slot would make
            // every rate below meaningless while still looking like eight images.
            let mut a = shown;
            let mut b = draw.selected_images;
            a.sort_unstable();
            b.sort_unstable();
            assert_eq!(a, b, "the shown set is not the selected set");
            assert_eq!(
                shown.iter().filter(|i| **i == target).count(),
                1,
                "the target must appear exactly once among the images shown"
            );

            for (p, image) in shown.iter().enumerate() {
                if *image == target {
                    by_position[p] += 1;
                }
            }
            if *shown.iter().min().unwrap() == target {
                lowest_index += 1;
            }
            if let Some(from_big) = shown.iter().find(|i| members[BIG].contains(i)) {
                big_shown += 1;
                if *from_big == target {
                    big_was_target += 1;
                }
            }
        }

        // Five standard deviations of a binomial at one in eight. Wide enough that a correct
        // implementation cannot trip it, narrow enough to catch a bias of half a percentage point.
        let within_chance = |hits: u32, trials: u32, what: &str| {
            let expected = trials as f64 / derive::SET_SIZE as f64;
            let sigma = (trials as f64 * (1.0 / 8.0) * (7.0 / 8.0)).sqrt();
            let off = (hits as f64 - expected).abs();
            assert!(
                off < 5.0 * sigma,
                "{what} scores {hits} of {trials}, chance is {expected:.0} (off by {:.1} sigma)",
                off / sigma
            );
        };

        for (p, &hits) in by_position.iter().enumerate() {
            within_chance(
                hits,
                ROUNDS as u32,
                &format!("always taking display position {p}"),
            );
        }
        within_chance(
            lowest_index,
            ROUNDS as u32,
            "always taking the lowest identifier",
        );
        within_chance(
            big_was_target,
            big_shown,
            "always taking the image from the largest category",
        );
    }

    /// FR-037: one evaluated answer per trial, whichever image the second one names.
    #[tokio::test]
    async fn a_second_answer_is_refused() {
        let state = with_min_view(&state_with_pool(), 0);
        let token = holder(&state);
        let (trial_token, images) = revealed(&state, &token).await;

        let first = call(
            &state,
            post(
                "/api/trial/answer",
                &token,
                serde_json::json!({ "token": trial_token, "chosen": images[0] }),
            ),
        )
        .await;
        assert_eq!(first.status(), StatusCode::OK);

        // The replay D16 describes: resubmit with the next image and read the verdict.
        let second = call(
            &state,
            post(
                "/api/trial/answer",
                &token,
                serde_json::json!({ "token": trial_token, "chosen": images[1] }),
            ),
        )
        .await;
        assert_eq!(second.status(), StatusCode::CONFLICT);
        assert_eq!(resolves(&state), 1);
    }

    /// The open-trial cap of D17 was removed by the operator. This is the test that keeps it
    /// removed: it fails the moment a request starts counting what the account already holds.
    #[tokio::test]
    async fn an_account_may_hold_as_many_open_trials_as_it_likes() {
        let state = with_min_view(&state_with_pool(), 0);
        let token = holder(&state);

        for _ in 0..12 {
            let response = call(&state, post("/api/trial", &token, serde_json::json!({}))).await;
            assert_eq!(response.status(), StatusCode::CREATED);
        }

        assert_eq!(
            state.db.head().unwrap().0,
            12,
            "every start is a commit in the permanent record"
        );
    }

    /// The reveal payload is what a third party recomputes the trial from, so it has to be the
    /// payload that was actually used — not a re-draw, and not the client's word for it.
    #[tokio::test]
    async fn the_answer_publishes_the_randomness_that_produced_the_target() {
        let state = with_min_view(&state_with_pool(), 0);
        let token = holder(&state);
        let (trial_token, images) = revealed(&state, &token).await;

        let response = call(
            &state,
            post(
                "/api/trial/answer",
                &token,
                serde_json::json!({ "token": trial_token, "chosen": images[3] }),
            ),
        )
        .await;
        let body = json(response).await;

        let s_server = base64_bytes(body["s_server"].as_str().unwrap()).unwrap();
        let s_client = base64_bytes(body["s_client"].as_str().unwrap()).unwrap();
        let nonce = base64_bytes(body["nonce"].as_str().unwrap()).unwrap();
        assert_eq!(
            s_client,
            vec![9u8; SEED_BYTES],
            "the client's own contribution comes back"
        );

        // What the browser does, and what anyone holding the export can do (FR-022, SC-002).
        let start = state.db.entries_from(1, 1).unwrap().remove(0);
        let Body::Commit {
            coordinate,
            commitment: published,
            ..
        } = start.body
        else {
            panic!("the first entry is the commit")
        };
        assert!(crate::trial::commit::verify(
            &s_server,
            &nonce,
            &coordinate,
            &published
        ));

        let draw = derive::derive(&s_server, &s_client, &state.pool.members()).unwrap();
        let target = image_id(&state.pool, draw.target_image()).unwrap();
        assert_eq!(body["target"], target);
        assert_eq!(body["hit"], images[3] == target);
        assert_eq!(
            body["seq"], 2,
            "the sequence number names the resolve entry in the public log"
        );
    }

    /// Every endpoint here writes to a permanent record under an account. An unauthenticated
    /// caller must not reach any of them.
    #[tokio::test]
    async fn the_loop_is_closed_to_strangers() {
        let state = state_with_pool();
        for (uri, body) in [
            ("/api/trial", serde_json::json!({})),
            (
                "/api/trial/reveal",
                serde_json::json!({ "token": "1.x", "s_client": "" }),
            ),
            (
                "/api/trial/answer",
                serde_json::json!({ "token": "1.x", "chosen": "img_000" }),
            ),
        ] {
            let request = Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(AxumBody::from(body.to_string()))
                .unwrap();
            let response = call(&state, request).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri} is open");
        }
        assert_eq!(state.db.head().unwrap().0, 0);
    }

    #[test]
    fn the_token_carries_its_sequence_number_in_the_open() {
        assert_eq!(decompose(&compose(42, "abc")), Some((42, "abc")));
        assert_eq!(decompose("abc"), None, "a token without a sequence number");
        assert_eq!(
            decompose("x.abc"),
            None,
            "a sequence number that is not one"
        );
    }

    #[test]
    fn a_coordinate_has_the_shape_the_commitment_binds() {
        for _ in 0..100 {
            let c = coordinate();
            assert_eq!(c.len(), 9);
            assert_eq!(c.as_bytes()[4], b'-');
            assert!(c.bytes().filter(|b| b.is_ascii_digit()).count() == 8);
        }
    }
}
