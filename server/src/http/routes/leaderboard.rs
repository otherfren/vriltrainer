//! `GET /api/leaderboard`, sorted by the Wilson lower bound, which is also the
//! figure displayed: a board sorted by an invisible statistic produces endless argument (D20).

use axum::Router;

use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    // T057.
    Router::new()
}
