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

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::connect_info::IntoMakeServiceWithConnectInfo;
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
///
/// Layer order is deliberate: the correlation identifier and the request span are outermost, so
/// the locale warning and every handler line carry them.
pub fn router(state: AppState) -> Router {
    let locale = state.config.locale;
    trace::instrument(locale::announce(routes::all().with_state(state), locale))
}

/// What [`axum::serve()`] should be handed.
///
/// The make-service rather than the [`Router`], because `ConnectInfo` is the only way a handler
/// learns which peer it is talking to, and without the peer the forwarded client address is either
/// refused from the proxy or believed from anyone (R8, [`client_addr`]). Serving the bare router
/// compiles and runs; it simply makes the per-address limit meaningless, which is the failure mode
/// R8 exists to keep out of the deployment.
pub fn service(state: AppState) -> IntoMakeServiceWithConnectInfo<Router, SocketAddr> {
    router(state).into_make_service_with_connect_info::<SocketAddr>()
}

/// Fixtures for the route tests. Every handler needs the whole [`AppState`], so building one is
/// the first thing each of those tests would otherwise do.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::pool::{ImageEntry, Manifest};

    /// State over an empty in-memory database and a two-image manifest.
    ///
    /// The manifest is too small to draw a trial from — that needs eight categories — so a test
    /// that derives anything must build its own. It exists here only so the pool is a validated
    /// one rather than a hash that does not match its contents.
    pub(crate) fn state() -> AppState {
        let categories = vec!["a".to_string(), "b".to_string()];
        let images = vec![
            ImageEntry {
                id: "img_1".into(),
                category: "a".into(),
            },
            ImageEntry {
                id: "img_2".into(),
                category: "b".into(),
            },
        ];
        let manifest = Manifest {
            version: 1,
            manifest_hash: Manifest::compute_hash(&categories, &images),
            categories,
            images,
        };
        AppState {
            db: Arc::new(crate::db::Db::open_in_memory().expect("an in-memory database opens")),
            config: Arc::new(Config::default()),
            sealer: Arc::new(Sealer::new(&[7u8; 32])),
            pool: Arc::new(manifest),
        }
    }
}
