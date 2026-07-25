//! `POST /api/trial`, `/reveal` and `/answer` — the loop the product is.

use axum::Router;

use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    // T028, T029, T030.
    Router::new()
}
