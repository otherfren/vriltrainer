//! `GET /api/health` — what the uptime monitor polls.
//!
//! It reads the chain head rather than returning a constant. A process can accept connections
//! while the database underneath it has gone — a restore in progress, a full disk, ownership
//! changed by a deploy — and a check that only proves the socket is open reports that as healthy.
//! Nothing here is secret: the head is published at `GET /api/log/head` anyway.

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use serde::Serialize;

use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/health", get(health))
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    /// The head sequence number, which also tells the operator the two processes are looking at
    /// the same file.
    seq: u64,
    locale: &'static str,
    pool_version: u32,
}

async fn health(State(state): State<AppState>) -> Response {
    match state.db.head() {
        Ok((seq, _)) => Json(Health {
            status: "ok",
            seq,
            locale: state.config.locale.code(),
            pool_version: state.pool.version,
        })
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "health check cannot read the chain head");
            // 503 rather than 500: it says "send traffic elsewhere", which is the one thing a
            // monitor and a proxy both know how to act on.
            (StatusCode::SERVICE_UNAVAILABLE, Json(Unavailable { status: "unavailable" }))
                .into_response()
        }
    }
}

#[derive(Serialize)]
struct Unavailable {
    status: &'static str,
}
