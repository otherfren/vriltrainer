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
use axum::response::{IntoResponse, Response};

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
/// Plainly `404` now. This used to answer `501` for the handful of paths `contracts/http-api.md`
/// named before anyone had mounted them — so that an integrator reading a "not implemented" went
/// to the table of what exists rather than hunting for a typo. `DELETE /api/account/name` was the
/// last of them, and a `501` for a path that answers would be the more expensive lie of the two.
async fn unrouted() -> Response {
    ApiError::NotFound.into_response()
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

    /// The name review lives at `/admin`, so `/api/admin/...` is a wrong address rather than an
    /// unfinished one and has to say `404` — it is not a route anybody should be sent looking for
    /// under `/api`, and the prefix is the thing they got wrong.
    #[tokio::test]
    async fn the_admin_api_is_not_under_api() {
        let req = Request::builder()
            .uri("/api/admin/names")
            .body(Body::empty())
            .unwrap();
        let response = router(test_support::state()).oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
