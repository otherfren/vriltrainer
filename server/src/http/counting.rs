//! The traffic counters that can be read off a request rather than out of a handler (FR-052, D28).
//!
//! Page views, unique visitors, opened proofs and log downloads are all "somebody asked for this
//! address", so they belong in one place rather than scattered through four modules. Everything
//! that is an *outcome* — an account created, a trial completed, a name approved — is counted in
//! the handler that decided it, because only the handler knows whether it happened.
//!
//! Nothing here is written to the database on the request path. [`crate::metrics::Metrics`] holds
//! integers in memory and [`crate::tasks::spawn_metrics_flush`] writes them, so a page view never
//! waits on the lock that appends to the audit log.

use axum::extract::{FromRequestParts, Request};
use axum::middleware::Next;
use axum::response::Response;

use crate::db::now_rfc3339;
use crate::http::AppState;
use crate::http::client_addr::ClientAddr;
use crate::metrics::name;

/// Counts what the request line alone says.
pub async fn count(state: AppState, request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let is_page = is_page_view(&path);

    // Before the handler, because the address extractor wants the request parts and the response
    // does not carry them. Only page views count towards uniques: a visitor who loads one page and
    // plays fifty trials is one visitor, and counting every asset would make the figure a measure
    // of how many images the page has.
    if is_page {
        let (mut parts, body) = request.into_parts();
        let now = now_rfc3339();
        // Infallible by construction: the extractor warns and falls back rather than refusing,
        // because a request with no peer address is an operator fault and not a visitor's problem.
        let Ok(ClientAddr(addr)) = ClientAddr::from_request_parts(&mut parts, &state).await;
        state.metrics.saw(addr, &now);
        state.metrics.count(name::PAGE_VIEW, &now);
        if is_proof(parts.uri.path()) {
            state.metrics.count(name::PROOF_OPENED, &now);
        }
        return next.run(Request::from_parts(parts, body)).await;
    }

    let response = next.run(request).await;
    // Only a download that produced something. A 404 or a range nobody answered is not a copy of
    // the record leaving the building, and D12's bus factor is about copies that exist.
    if is_log_download(&path) && response.status().is_success() {
        state.metrics.count(name::LOG_DOWNLOAD, &now_rfc3339());
    }
    response
}

/// A document the browser is showing a person, rather than something the page then fetched.
///
/// Prefix matching only, and only against fixed prefixes this file names — no part of the path is
/// stored anywhere. `/api` is the client talking to the server, `/pool` is image bytes, and
/// `/admin` is the review surface rather than the site.
fn is_page_view(path: &str) -> bool {
    !(under(path, "/api") || under(path, "/pool") || under(path, "/admin"))
}

/// The verification panel: a reader who came to check a trial rather than to play one (FR-024,
/// D12). Worth counting separately because it is the one number that says whether the
/// verifiability the whole site is built on is used by anybody.
fn is_proof(path: &str) -> bool {
    under(path, "/verify")
}

/// The export, and not `/api/log/head` — a client polls the head, and a poll is not a download.
/// The paged export is `/api/log?from=…`, and `Uri::path()` excludes the query, so the whole of
/// that route is this one string.
fn is_log_download(path: &str) -> bool {
    path == "/api/log"
}

/// Prefix matching on a segment boundary, so a client route called `/apiary` is not `/api`.
fn under(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_what_a_person_is_looking_at() {
        for path in ["/", "/trial", "/leaderboard", "/verify/abc", "/apiary"] {
            assert!(is_page_view(path), "{path} should count as a page view");
        }
        for path in ["/api", "/api/trial", "/pool/img_1.png", "/admin/names"] {
            assert!(!is_page_view(path), "{path} is not a page view");
        }
    }

    #[test]
    fn the_head_is_polled_and_the_export_is_downloaded() {
        assert!(is_log_download("/api/log"));
        assert!(!is_log_download("/api/log/head"));
    }

    #[test]
    fn the_verification_panel_is_counted_on_its_own() {
        assert!(is_proof("/verify"));
        assert!(is_proof("/verify/7f3a"));
        assert!(!is_proof("/verifiable"));
    }

    /// End to end through the real stack: a request arrives, and the figure is in the table after
    /// a flush. Counting that only ever happens in a unit test is a dashboard of zeroes.
    #[tokio::test]
    async fn a_request_through_the_router_reaches_the_table() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let state = crate::http::test_support::state();
        let request = Request::builder()
            .uri("/api/account")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from("{\"name\":\"otherfren\"}"))
            .unwrap();
        crate::http::router(state.clone())
            .oneshot(request)
            .await
            .unwrap();

        state.metrics.flush(&state.db, &now_rfc3339()).unwrap();
        let counted: i64 = state
            .db
            .reader()
            .unwrap()
            .query_row(
                "SELECT count FROM daily_metric WHERE metric = ?1",
                [name::ACCOUNT_CREATED],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(counted, 1);
    }
}
