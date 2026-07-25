//! `GET /api/log` and `GET /api/log/head` — the public record (FR-025).
//!
//! Unauthenticated on purpose. Copies held by third parties are the redundancy that partly
//! substitutes for the anchor deferred in D4.

use axum::Router;

use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    // T053.
    Router::new()
}
