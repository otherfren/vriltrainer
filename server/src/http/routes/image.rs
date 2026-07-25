//! `GET /pool/{file}` — the image bytes, out of the binary (D29).
//!
//! Not under `/api`: this is what an `<img src>` points at, and the client builds that address from
//! the manifest's identifiers alone (`/pool/<image_id>.png`). Nothing else connects a published
//! identifier to a picture, which is deliberate — the manifest carries no filenames, so there is no
//! second name for an image to be published under.
//!
//! The response is immutable in the strongest available sense. An identifier is the hash of the
//! bytes served under it, so a cached copy cannot go stale: either the bytes are the ones the id
//! names or they are somebody else's forgery, and re-fetching would not tell the difference. That
//! makes a year-long `immutable` directive a statement of fact rather than the usual optimistic
//! guess, and it matters here beyond the bandwidth — eight images load at the top of every trial,
//! and a visitor who plays fifty of them should be paying for the pool once.

use axum::Router;
use axum::extract::Path as UrlPath;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::http::{ApiError, AppState};
use crate::pool::embedded;

/// A year, which is what `max-age` is conventionally capped at. `immutable` is the operative half.
const A_YEAR: &str = "public, max-age=31536000, immutable";

pub fn routes() -> Router<AppState> {
    Router::new().route("/pool/{file}", get(image))
}

async fn image(UrlPath(file): UrlPath<String>) -> Response {
    // The extension is part of the address the client builds, and stripping it is the whole of the
    // parsing. Anything else — a path segment, a query, a different extension — is simply not an
    // image this build carries, and says so with a 404 rather than a guess.
    let Some(id) = file.strip_suffix(".png") else {
        return ApiError::NotFound.into_response();
    };
    let Some(bytes) = embedded::get(id) else {
        return ApiError::NotFound.into_response();
    };

    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, A_YEAR),
        ],
        // The id is a hash of the body, so it is already a strong validator — quoting it is all an
        // ETag needs. A conditional request then costs a header instead of 270 kB.
        [(header::ETAG, format!("\"{id}\""))],
        bytes,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::http::{router, test_support};
    use crate::pool::embedded;

    async fn call(uri: &str) -> axum::response::Response {
        let state = test_support::state();
        router(state)
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    /// Served from the binary, with the headers that make a content-addressed body cacheable
    /// forever. Skipped on a checkout with no images, for the reason `embedded`'s tests are.
    #[tokio::test]
    async fn an_embedded_image_is_served_with_its_hash_as_the_validator() {
        let Some((id, bytes)) = embedded::all().first() else {
            return;
        };
        let response = call(&format!("/pool/{id}.png")).await;
        assert_eq!(response.status(), StatusCode::OK);

        let headers = response.headers();
        assert_eq!(headers["content-type"], "image/png");
        assert!(
            headers["cache-control"]
                .to_str()
                .unwrap()
                .contains("immutable")
        );
        assert_eq!(headers["etag"], format!("\"{id}\""));

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), *bytes);
    }

    /// An id this build does not carry is a 404 and not an empty 200 — a zero-length image renders
    /// as a broken picture and reads as a client fault.
    #[tokio::test]
    async fn an_unknown_image_is_not_found() {
        let missing = call("/pool/img_0000000000000000000000000000ffff.png").await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        // No extension, and an extension nobody serves.
        assert_eq!(call("/pool/img_00").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(call("/pool/x.jpg").await.status(), StatusCode::NOT_FOUND);
    }
}
