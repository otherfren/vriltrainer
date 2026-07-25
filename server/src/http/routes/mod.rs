//! One module per resource, each mounting its own paths.
//!
//! Splitting by resource rather than by verb keeps a route defined in exactly one place, which
//! matters because `contracts/http-api.md` is the contract and a route that exists in two files
//! drifts from it in one of them.

pub mod account;
pub mod admin;
pub mod handoff;
pub mod health;
pub mod image;
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
        .merge(image::routes())
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

/// The contracted paths that are still unmounted — nothing else belongs here.
///
/// Every other route in `contracts/http-api.md` now has a module, and an entry for a mounted path
/// can never be reached: the real route wins, so it would sit here reading like unfinished work
/// long after the work was finished. `DELETE /api/account/name` (FR-035) is the last one left.
///
/// The name review is deliberately absent even though it is contracted. It is mounted under
/// `/admin`, not `/api/admin` — an entry for the latter would promise a `501` "coming soon" for an
/// address that will never exist, and send an integrator looking for a route instead of at the
/// prefix they got wrong.
fn contracted(path: &str) -> bool {
    path == "/api/account/name"
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

    /// A mounted path must never also be listed as unbuilt. The listing is unreachable for one —
    /// the route wins — so the mistake is invisible in behaviour and survives until someone reads
    /// the contract and the file side by side.
    #[test]
    fn nothing_already_mounted_is_advertised_as_unbuilt() {
        for mounted in [
            "/api/account",
            "/api/trial",
            "/api/trial/reveal",
            "/api/trial/answer",
            "/api/stats/me",
            "/api/stats/aggregate",
            "/api/leaderboard",
            "/api/log",
            "/api/log/head",
            "/api/pool/1/manifest",
            "/api/handoff",
            "/api/handoff/redeem",
        ] {
            assert!(!super::contracted(mounted), "{mounted} is mounted");
        }
    }

    /// The name review lives at `/admin`, so `/api/admin/...` is a wrong address rather than an
    /// unfinished one and has to say `404`. A `501` there reads as "not built yet" for a path that
    /// is never going to be built, which is the more expensive of the two lies.
    #[tokio::test]
    async fn the_admin_api_is_not_under_api() {
        assert!(!super::contracted("/api/admin/names"));

        let req = Request::builder()
            .uri("/api/admin/names")
            .body(Body::empty())
            .unwrap();
        let response = router(test_support::state()).oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
