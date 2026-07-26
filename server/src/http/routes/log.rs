//! `GET /api/log` and `GET /api/log/head` — the public record (FR-025).
//!
//! Unauthenticated on purpose. Copies held by third parties are the redundancy that partly
//! substitutes for the anchor deferred in D4.
//!
//! The two endpoints are separate because a head alone proves nothing — nobody can check that the
//! entries beneath it agree with it — and an export alone proves nothing either, because the
//! operator serving it also chose what went into it (D13). Together they work: readers who pulled
//! the head yesterday and the file today can tell whether the two agree, and readers comparing
//! heads with each other are what would catch a divergent second log — the gap
//! `contracts/public-log.md` states plainly and does not close.

use axum::Router;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};

use crate::db::now_rfc3339;
use crate::http::{ApiError, AppState};
use crate::log::export;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/log", get(download))
        .route("/api/log/head", get(head))
}

#[derive(Deserialize)]
struct Range {
    from: Option<u64>,
    limit: Option<u64>,
}

#[derive(Serialize)]
struct Head {
    seq: u64,
    entry_hash: String,
    /// When this answer was produced. A head with no time on it cannot be compared against the
    /// head somebody else pulled, and comparing heads is why it is published separately at all.
    as_of: String,
}

/// The current head of the chain.
async fn head(State(state): State<AppState>) -> Result<Response, ApiError> {
    let (seq, entry_hash) = state.db.head()?;
    Ok(Json(Head {
        seq,
        entry_hash,
        as_of: now_rfc3339(),
    })
    .into_response())
}

/// A page of the record, newline-delimited JSON.
///
/// Paging is by sequence number rather than by offset because entries are immutable and numbered
/// without gaps: the same `from` returns the same lines forever, so a downloader interrupted at
/// entry 40,000 resumes there instead of starting again. It also means a page can be *finished*,
/// which the cache rule below depends on.
async fn download(
    State(state): State<AppState>,
    Query(range): Query<Range>,
) -> Result<Response, ApiError> {
    // Sequence numbers start at 1, so `from=0` is the same request as `from=1`. Treated as its own
    // case it would hand back a first page one entry shorter than everybody else's.
    let from = range.from.unwrap_or(1).max(1);
    let limit = export::page_limit(range.limit);

    let entries = state.db.entries_from(from, limit)?;

    // A full page can never change: `limit` immutable entries from a fixed sequence number. A
    // short page is short only because the log has not caught up yet, and a cache holding one
    // serves a truncated record to everyone behind it — the failure this endpoint exists to make
    // impossible. The two cases therefore get opposite answers.
    let cache = if entries.len() as u64 == limit {
        "public, max-age=31536000, immutable"
    } else {
        "no-store"
    };

    Ok((
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::CACHE_CONTROL, cache),
        ],
        export::to_ndjson(&entries),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use axum::response::Response;
    use tower::ServiceExt;

    use crate::db::now_rfc3339;
    use crate::http::{AppState, router, test_support};
    use crate::log::chain::{Body as EntryBody, GENESIS};
    use crate::log::export;

    fn commit(trial: &str) -> EntryBody {
        EntryBody::Commit {
            trial: trial.into(),
            account: "acct".into(),
            coordinate: "4821-9037".into(),
            commitment: "sha256:aa".into(),
            pool_version: 1,
            pool_manifest_hash: Some("sha256:pool".into()),
        }
    }

    /// A state whose log already holds `n` commits, written through the real append path — so the
    /// chain the export serves is the chain the append discipline produced, not a fixture.
    fn state_with(n: usize) -> AppState {
        let state = test_support::state();
        state
            .db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO account (id, public_id, token_hash, created_at)
                     VALUES ('acct', 'ACCT01', 'hash', ?1)",
                    rusqlite::params![now_rfc3339()],
                )?;
                Ok(())
            })
            .unwrap();
        for i in 0..n {
            state
                .db
                .append(&now_rfc3339(), commit(&format!("t{i}")))
                .unwrap();
        }
        state
    }

    async fn get(state: &AppState, uri: &str) -> Response {
        let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
        router(state.clone()).oneshot(request).await.unwrap()
    }

    async fn text(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn an_empty_log_publishes_the_genesis_head() {
        let response = get(&test_support::state(), "/api/log/head").await;
        assert_eq!(response.status(), StatusCode::OK);
        let json: serde_json::Value = serde_json::from_str(&text(response).await).unwrap();
        assert_eq!(json["seq"], 0);
        assert_eq!(json["entry_hash"], GENESIS);
        assert!(json["as_of"].as_str().unwrap().ends_with('Z'));
    }

    #[tokio::test]
    async fn the_head_names_the_last_line_of_the_export() {
        let state = state_with(3);

        let head: serde_json::Value =
            serde_json::from_str(&text(get(&state, "/api/log/head").await).await).unwrap();
        let entries = export::read_ndjson(&text(get(&state, "/api/log").await).await).unwrap();

        let last = entries.last().unwrap();
        assert_eq!(head["seq"], last.seq);
        assert_eq!(
            head["entry_hash"], last.hash,
            "a head that does not name the last line is a head nobody can check"
        );
    }

    #[tokio::test]
    async fn a_page_starts_where_it_was_asked_to_and_is_ndjson() {
        let state = state_with(5);
        let response = get(&state, "/api/log?from=3&limit=2").await;
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/x-ndjson"
        );
        let entries = export::read_ndjson(&text(response).await).unwrap();
        let seqs: Vec<u64> = entries.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![3, 4]);
    }

    #[tokio::test]
    async fn from_zero_is_the_same_request_as_from_one() {
        let state = state_with(3);
        assert_eq!(
            text(get(&state, "/api/log?from=0").await).await,
            text(get(&state, "/api/log?from=1").await).await
        );
    }

    /// A short page is short only because the log has not caught up. Cached, it becomes a
    /// permanently truncated copy of the record for everyone behind that cache.
    #[tokio::test]
    async fn an_unfinished_page_is_not_cacheable_and_a_finished_one_is() {
        let state = state_with(3);

        let short = get(&state, "/api/log?limit=10").await;
        assert_eq!(
            short.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );

        let full = get(&state, "/api/log?limit=3").await;
        assert!(
            full.headers()
                .get(header::CACHE_CONTROL)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("immutable")
        );
    }

    /// The record is what strangers audit, so it cannot sit behind a token (D13, FR-025). This is
    /// what fails if authentication is ever added above the router.
    #[tokio::test]
    async fn the_record_is_readable_without_a_token() {
        let state = state_with(1);
        for uri in ["/api/log", "/api/log/head"] {
            assert_eq!(get(&state, uri).await.status(), StatusCode::OK, "{uri}");
        }
    }
}
