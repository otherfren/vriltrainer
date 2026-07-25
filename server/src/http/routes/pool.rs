//! `GET /api/pool/{version}/manifest`.
//!
//! Served for **every** version for as long as the service runs: a trial recorded under v1 stays
//! verifiable only while v1's manifest still answers (D5).

use axum::Router;

use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    // T054.
    Router::new()
}
