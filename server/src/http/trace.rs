//! Request logging.
//!
//! One rule governs everything here: **the access token must never reach a log line** (FR-006).
//! It lives in the URL fragment precisely because fragments are not transmitted, so the server
//! cannot log one by accident today — but a handler that ever accepts a token in a path or query
//! would put it into every access log at once, and this is where that would show up.

use axum::Router;
use tower_http::trace::TraceLayer;

/// Wraps the router in request tracing.
///
/// Takes the router rather than returning a layer so that T025 can change what the layer *is* — a
/// correlation-identifier span, a different failure classifier — without the signature moving and
/// dragging `http::router` with it.
pub fn instrument(router: Router) -> Router {
    router.layer(TraceLayer::new_for_http())
}
