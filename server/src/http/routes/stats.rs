//! `GET /api/stats/me` and `GET /api/stats/aggregate`.

use axum::Router;

use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    // T042, T043.
    Router::new()
}
