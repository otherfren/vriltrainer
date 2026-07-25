//! The HTTP surface, per `specs/001-remote-viewing-trainer/contracts/http-api.md`.
//!
//! Two things about this layer are decisions rather than plumbing. The locale is fixed at startup
//! and never read off a request (D24, [`locale`]), and the client address is only believed from
//! the proxy (R8, [`client_addr`]) — miss either and a decision breaks without anything looking
//! wrong.

pub mod client_addr;
pub mod locale;
pub mod routes;
pub mod trace;

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::config::Config;
use crate::db::Db;
use crate::pool::Manifest;
use crate::trial::token::Sealer;

/// Everything a handler is allowed to reach. Cheap to clone: axum clones it per request.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub config: Arc<Config>,
    /// Seals the trial tokens of D16, so `s_server` never reaches the database and a backup
    /// carries no pending answers.
    pub sealer: Arc<Sealer>,
    /// The published manifest this process draws against, validated at startup. Every trial
    /// records the hash it was drawn under, so a manifest whose hash is wrong would publish
    /// trials nobody can recompute.
    pub pool: Arc<Manifest>,
}

/// 425, the status the contract gives an answer submitted too soon after reveal. Spelled out
/// because `http` carries no constant for it.
fn too_early() -> StatusCode {
    StatusCode::from_u16(425).expect("425 is a valid status code")
}

/// What a handler returns when it refuses.
///
/// The status codes are the contract, not an implementation detail — the client distinguishes
/// "too fast" from "already answered" from "expired" and says different things about each. The
/// body is deliberately terse and carries no server detail.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,
    /// No access token, or one that matches no account. There is no recovery path to offer (D9).
    #[error("unauthorized")]
    Unauthorized,
    #[error("bad request")]
    BadRequest(&'static str),
    /// The trial already has an evaluated answer (FR-037).
    #[error("already answered")]
    AlreadyAnswered,
    /// Answered less than the minimum viewing time after reveal. Returned **without the chosen
    /// image having been examined**, or the refusal becomes an oracle for the target (FR-039).
    #[error("too fast")]
    TooFast,
    /// The trial's validity has elapsed (FR-038), or a handoff code was already burnt (D11).
    #[error("gone")]
    Gone,
    #[error("rate limited")]
    RateLimited,
    #[error(transparent)]
    Db(#[from] crate::db::DbError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::BadRequest(why) => (StatusCode::BAD_REQUEST, *why),
            ApiError::AlreadyAnswered => (StatusCode::CONFLICT, "already answered"),
            ApiError::TooFast => (too_early(), "too fast"),
            ApiError::Gone => (StatusCode::GONE, "gone"),
            ApiError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate limited"),
            ApiError::Db(e) => {
                // Logged, never returned: a database message can name a table, a column or a
                // constraint, and none of that is the caller's business.
                tracing::error!(error = %e, "database failure");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            }
        };
        (status, axum::Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// The whole application. Route modules mount themselves; this only assembles them.
pub fn router(state: AppState) -> Router {
    trace::instrument(routes::all().with_state(state))
}
