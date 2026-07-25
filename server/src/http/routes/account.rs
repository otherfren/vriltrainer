//! `POST /api/account`, `DELETE /api/account/name`, and the rename of FR-048.

use axum::Router;

use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    // T027, T069, T100.
    Router::new()
}
