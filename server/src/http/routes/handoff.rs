//! `POST /api/handoff` and `POST /api/handoff/redeem` — the language switch (D11).

use axum::Router;

use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    // T066.
    Router::new()
}
