//! One module per resource, each mounting its own paths.
//!
//! Splitting by resource rather than by verb keeps a route defined in exactly one place, which
//! matters because `contracts/http-api.md` is the contract and a route that exists in two files
//! drifts from it in one of them.

pub mod account;
pub mod admin;
pub mod handoff;
pub mod health;
pub mod leaderboard;
pub mod log;
pub mod pool;
pub mod stats;
pub mod trial;

use axum::Router;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};

use super::{ApiError, AppState};

pub fn all() -> Router<AppState> {
    Router::new()
        .merge(account::routes())
        .merge(admin::routes())
        .merge(handoff::routes())
        .merge(health::routes())
        .merge(leaderboard::routes())
        .merge(log::routes())
        .merge(pool::routes())
        .merge(stats::routes())
        .merge(trial::routes())
        .fallback(unrouted)
}

/// Anything no module claimed.
///
/// A path the contract names but nobody has mounted yet answers `501`, not `404`. The client is
/// built against the same contract, and a `404` there reads as "wrong address" and sends whoever
/// is integrating to look for a typo instead of at a table of what exists. A real route always
/// wins over this, so an entry below becomes inert the moment its module mounts the path — and the
/// list goes with the last of them.
async fn unrouted(uri: Uri) -> Response {
    if contracted(uri.path()) {
        let body = Json(serde_json::json!({ "error": "not implemented" }));
        return (StatusCode::NOT_IMPLEMENTED, body).into_response();
    }
    ApiError::NotFound.into_response()
}

fn contracted(path: &str) -> bool {
    const PATHS: [&str; 11] = [
        "/api/account",
        "/api/account/name",
        "/api/trial",
        "/api/trial/reveal",
        "/api/trial/answer",
        "/api/stats/me",
        "/api/stats/aggregate",
        "/api/leaderboard",
        "/api/log",
        "/api/log/head",
        "/api/handoff",
    ];

    PATHS.contains(&path)
        || path == "/api/handoff/redeem"
        // `GET /api/pool/{version}/manifest`, served for every version there has ever been (D5).
        || (path.starts_with("/api/pool/") && path.ends_with("/manifest"))
        // The name review of D25. Reversible operations only, so the surface is small by design.
        || path.starts_with("/api/admin/")
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;

    use crate::http::{router, test_support};

    #[tokio::test]
    async fn health_reports_the_head_and_the_locale() {
        let response = router(test_support::state())
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        // D24: the language is the startup flag, so it is the same answer whatever was asked for.
        assert_eq!(
            response.headers().get(header::CONTENT_LANGUAGE).unwrap(),
            "de"
        );

        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["seq"], 0);
        assert_eq!(json["locale"], "de");
    }

    /// The path here has to be one nobody has mounted yet, and each one stops being that as its
    /// module lands — `/api/trial` was this test's subject until the trial loop was built, and
    /// `/api/stats/me` until the statistics did. When the last of them is mounted, this test and
    /// [`contracted`] go together.
    #[tokio::test]
    async fn a_contracted_path_that_is_not_built_yet_says_so() {
        let req = Request::builder()
            .method("GET")
            .uri("/api/account/name")
            .body(Body::empty())
            .unwrap();
        let response = router(test_support::state()).oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn anything_else_is_not_found() {
        let response = router(test_support::state())
            .oneshot(
                Request::builder()
                    .uri("/api/nonsense")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn the_versioned_manifest_path_is_contracted_for_every_version() {
        assert!(super::contracted("/api/pool/1/manifest"));
        assert!(super::contracted("/api/pool/97/manifest"));
        assert!(!super::contracted("/api/pool/1"));
    }
}
