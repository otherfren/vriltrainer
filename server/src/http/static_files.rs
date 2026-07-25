//! Serving the built client bundle.
//!
//! nginx proxies **every** path on the domain to this process (D17), so if the service does not
//! serve the application, nothing does — the API answers and the site is a blank page. That is the
//! failure this module exists to prevent, and it is invisible in every test that only calls
//! `/api`.
//!
//! Two rules govern the dispatch:
//!
//! - `/api` and `/admin` are answered by the router, always. A misspelled endpoint has to keep
//!   returning the 404 or 501 that `routes::unrouted` decided on; serving `index.html` with a 200
//!   in its place turns a wrong address into a client that parses a web page as JSON.
//! - everything else falls through to the bundle, and anything the bundle does not have falls
//!   through to `index.html`. Angular routes on the path (D9 requires `PathLocationStrategy`,
//!   because the access token lives in the fragment), so a reload of `/stats` reaches the server
//!   as a path that has no file behind it and must still return the application.

use std::path::Path;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

/// Wraps `router` so that non-API paths are served from `public`.
///
/// A layer rather than a fallback: the router already has one — the 501 that tells a client a
/// contracted endpoint is not built yet — and replacing it would take that answer away.
pub fn mount(router: Router, public: Option<&Path>) -> Router {
    let Some(dir) = public else {
        // No `--public`. The API still works, which is what the tests and a headless deployment
        // need, and an operator who meant to serve the client gets a 404 rather than a blank page.
        tracing::warn!("no --public directory: this process serves the API only");
        return router;
    };

    let index = dir.join("index.html");
    if !index.is_file() {
        // Refused loudly at startup rather than per request. A bundle without an `index.html` is
        // a deploy that copied the wrong directory, and every page would answer 404 for it.
        tracing::error!(
            public = %dir.display(),
            "the public directory has no index.html; the client will not load"
        );
    }
    let files = ServeDir::new(dir).fallback(ServeFile::new(index));

    router.layer(axum::middleware::from_fn(
        move |request: Request, next: Next| {
            let files = files.clone();
            async move {
                if is_service_path(request.uri().path()) {
                    return next.run(request).await;
                }
                match files.oneshot(request).await {
                    Ok(response) => response.map(Body::new),
                    // `ServeDir`'s error type is `Infallible`: a missing file is a 404 response,
                    // not an error.
                    Err(never) => match never {},
                }
            }
        },
    ))
}

/// Paths the router owns. Prefix matching on a segment boundary, so a client route called
/// `/apiary` is still a page.
fn is_service_path(path: &str) -> bool {
    ["/api", "/admin"].iter().any(|p| {
        path == *p
            || path
                .strip_prefix(p)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::http::{AppState, router, test_support};

    #[test]
    fn the_router_keeps_its_own_prefixes() {
        assert!(is_service_path("/api"));
        assert!(is_service_path("/api/trial"));
        assert!(is_service_path("/admin/names"));
        // Client routes, which happen to start with the same letters.
        assert!(!is_service_path("/apiary"));
        assert!(!is_service_path("/administrivia"));
        assert!(!is_service_path("/"));
        assert!(!is_service_path("/stats"));
    }

    /// A directory of the shape `ng build` produces, removed when the test ends.
    struct Bundle(PathBuf);

    impl Bundle {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!("vriltrainer-public-{}-{tag}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("index.html"), "<title>vriltrainer</title>").unwrap();
            std::fs::write(path.join("main.js"), "console.log(1)").unwrap();
            Bundle(path)
        }
    }

    impl Drop for Bundle {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn state_serving(bundle: &Bundle) -> AppState {
        let base = test_support::state();
        let mut config = (*base.config).clone();
        config.public_dir = Some(bundle.0.clone());
        AppState {
            config: Arc::new(config),
            ..base
        }
    }

    async fn get(state: &AppState, uri: &str) -> (StatusCode, String) {
        let response = router(state.clone())
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    #[tokio::test]
    async fn the_bundle_is_served_and_unknown_paths_are_the_application() {
        let bundle = Bundle::new("served");
        let state = state_serving(&bundle);

        let (status, body) = get(&state, "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("vriltrainer"));

        let (status, body) = get(&state, "/main.js").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "console.log(1)");

        // The reload case: a client route with no file behind it is still the application.
        let (status, body) = get(&state, "/stats").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("vriltrainer"));
    }

    /// The rule that makes this safe to mount: the API keeps answering for itself, including for
    /// addresses it does not serve.
    #[tokio::test]
    async fn the_api_still_routes_first() {
        let bundle = Bundle::new("api-first");
        let state = state_serving(&bundle);

        let (status, body) = get(&state, "/api/health").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("\"status\":\"ok\""));

        let (status, _) = get(&state, "/api/nonsense").await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "a wrong API address must not answer with the web page"
        );
    }

    /// Without the flag the service is exactly what it was: an API and a 404 for everything else.
    #[tokio::test]
    async fn without_the_flag_nothing_is_served() {
        let state = test_support::state();
        assert_eq!(Config::default().public_dir, None);
        assert_eq!(get(&state, "/").await.0, StatusCode::NOT_FOUND);
    }
}
