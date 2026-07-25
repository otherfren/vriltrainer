//! The public admin API of D25: list pending names, approve, reject.
//!
//! **Reversible operations only.** Every destructive operation — deleting an account, touching the
//! log, changing pool versions — stays a CLI subcommand behind SSH, so a leaked admin key costs an
//! embarrassing name on the board for an hour and not the audit log. That bound is what lets this
//! API be public and have one privilege level instead of roles and scopes.

use axum::Router;

use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    // T098.
    Router::new()
}
