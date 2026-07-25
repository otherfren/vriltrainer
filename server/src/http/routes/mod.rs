//! One module per resource, each mounting its own paths.
//!
//! Splitting by resource rather than by verb keeps a route defined in exactly one place, which
//! matters because `contracts/http-api.md` is the contract and a route that exists in two files
//! drifts from it in one of them.

pub mod account;
pub mod admin;
pub mod handoff;
pub mod leaderboard;
pub mod log;
pub mod pool;
pub mod stats;
pub mod trial;

use axum::Router;

use super::AppState;

pub fn all() -> Router<AppState> {
    Router::new()
        .merge(account::routes())
        .merge(admin::routes())
        .merge(handoff::routes())
        .merge(leaderboard::routes())
        .merge(log::routes())
        .merge(pool::routes())
        .merge(stats::routes())
        .merge(trial::routes())
}
