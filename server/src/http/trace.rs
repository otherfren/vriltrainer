//! Request logging.
//!
//! One rule governs everything here: **the access token must never reach a log line** (FR-006).
//! It lives in the URL fragment precisely because fragments are not transmitted, so the server
//! cannot log one by accident today — but a handler that ever accepts a token in a path or query
//! would put it into every access log at once, and this is where that would show up.
//!
//! Three things follow, and none of them may be relaxed for convenience:
//!
//! - only the **matched route pattern** is logged, never the request target. `/admin/names/{id}/
//!   approve` is what goes in the line; the address that was actually called carries an account
//!   identifier in it (FR-051, D28). [`safe_target`] is the second cut, for the one field that is
//!   still derived from a path;
//! - no header is logged. `Authorization` carries the token itself and `Referer` carries whatever
//!   address the browser came from, which on this site is a page the user was about to share;
//! - what is logged instead is a correlation identifier, which names a request without describing
//!   it, so a user can quote it and an operator can find it.

use axum::Router;
use axum::extract::{MatchedPath, Request};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use rand::Rng;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{Level, Span};

use crate::config::Locale;

/// Returned on every response and accepted from the proxy, so one request can be followed across
/// nginx's log and this one.
pub const REQUEST_ID: &str = "x-request-id";

/// The correlation identifier of the request being handled, for handlers that want to name it in
/// an error they return.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Wraps the router in request tracing.
///
/// Takes the router rather than returning a layer so that what the layer *is* can change — a
/// different failure classifier, another field — without the signature moving and dragging
/// `http::router` with it.
pub fn instrument(router: Router, locale: Locale) -> Router {
    router
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(move |req: &axum::http::Request<_>| request_span(req, locale))
                // One line per request at the level the service actually runs at. The default is
                // DEBUG, which under the deployed filter means no access log at all.
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        // Outside the trace layer, so the identifier exists by the time the span is made.
        .layer(axum::middleware::from_fn(correlate))
}

async fn correlate(mut req: Request, next: Next) -> Response {
    let id = inbound_id(&req).unwrap_or_else(mint_id);
    req.extensions_mut().insert(RequestId(id.clone()));
    let mut response = next.run(req).await;
    let value = HeaderValue::from_str(&id).expect("an identifier here is ASCII by construction");
    response.headers_mut().insert(REQUEST_ID, value);
    response
}

/// An identifier the proxy set, if it is one we are willing to repeat.
///
/// The value is client-controlled whenever the proxy passes one through, and it is echoed into
/// both a log line and a response header. Restricting it to a short run of identifier characters
/// is what keeps someone from writing their own fields into the log.
fn inbound_id(req: &Request) -> Option<String> {
    let raw = req.headers().get(REQUEST_ID)?.to_str().ok()?;
    let ok = (1..=64).contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then(|| raw.to_owned())
}

fn mint_id() -> String {
    format!("{:016x}", rand::rng().random::<u64>())
}

fn request_span<B>(req: &axum::http::Request<B>, locale: Locale) -> Span {
    let id = req
        .extensions()
        .get::<RequestId>()
        .map_or("-", |r| r.0.as_str());
    tracing::info_span!(
        "request",
        id,
        method = %req.method(),
        route = matched_route(req),
        // Both processes write to the same journal (D24), so without this a line cannot be
        // attributed to a domain — and the two domains are the two audiences.
        locale = locale.code(),
    )
}

/// The pattern the router matched, or `-` when it matched nothing.
///
/// **Never the request target.** `/admin/names/{account_id}/approve` describes what was called
/// without saying who it was called about, and the target says both (FR-051, D28). The same is
/// true of any route that grows a path parameter later: a pattern cannot carry an identifier,
/// which is why the pattern is what is logged rather than a path with the known identifiers
/// scrubbed out of it.
///
/// A request that matched nothing is a static asset or a client route, and those go to `-` rather
/// than to their address for exactly the same reason — the log cannot tell in advance which
/// unmatched path is somebody's shared proof link. nginx's access log has the addresses, on the
/// short retention D28 sets, and that is the one place a visitor's address is written down.
fn matched_route<B>(req: &axum::http::Request<B>) -> &str {
    req.extensions()
        .get::<MatchedPath>()
        .map_or("-", |m| safe_target(m.as_str()))
}

/// The part of the request target that may be logged: the path, and nothing after it.
///
/// `Uri` drops the fragment while parsing and `Uri::path()` excludes the query, so both cuts are
/// already made by the time this is called. It cuts them again anyway, because the value being
/// protected is the one credential the product has (FR-006, D9) and the cost of being sure is one
/// `find`.
pub fn safe_target(path: &str) -> &str {
    match path.find(['?', '#']) {
        Some(end) => &path[..end],
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;
    use tracing_subscriber::fmt::MakeWriter;

    use super::*;

    /// The token as it would appear if any of this leaked it. Distinctive enough that a substring
    /// search over the whole log is a fair test.
    const SECRET: &str = "6f5c1d2e3a4b5c6d7e8f90a1b2c3d4e5";

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .expect("log buffer is not poisoned")
                .extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Captured {
        type Writer = Captured;
        fn make_writer(&'a self) -> Captured {
            self.clone()
        }
    }

    /// Runs one request through the real layer stack and returns the response together with
    /// everything that was logged while it ran.
    fn serve(req: Request<Body>) -> (Response, String) {
        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_max_level(Level::TRACE)
            .with_ansi(false)
            .finish();

        // A current-thread runtime, because the capturing subscriber is a thread-local default and
        // work moved to a worker thread would be logged somewhere this test cannot see.
        let response = tracing::subscriber::with_default(subscriber, || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime builds");
            rt.block_on(async {
                let app = instrument(
                    Router::new()
                        .route("/api/health", get(|| async { "ok" }))
                        // Stands in for the routes that carry an identifier in the address —
                        // `/admin/names/{account_id}/approve` is the real one.
                        .route("/api/thing/{id}", get(|| async { "ok" })),
                    Locale::De,
                );
                app.oneshot(req).await.expect("the router is infallible")
            })
        });

        let logged = String::from_utf8(captured.0.lock().unwrap().clone()).expect("logs are utf-8");
        (response, logged)
    }

    #[test]
    fn drops_everything_after_the_path() {
        assert_eq!(safe_target("/api/trial"), "/api/trial");
        assert_eq!(safe_target("/api/trial#t=secret"), "/api/trial");
        assert_eq!(safe_target("/api/trial?token=secret"), "/api/trial");
        assert_eq!(safe_target("/api/trial?a=1#t=secret"), "/api/trial");
    }

    #[test]
    fn no_fragment_or_query_reaches_the_log() {
        let uri = format!("/api/health?token={SECRET}#t={SECRET}");
        let req = Request::builder()
            .uri(&uri)
            .body(Body::empty())
            .expect("a valid request");
        let (response, logged) = serve(req);

        assert_eq!(response.status(), StatusCode::OK);
        // The positive control: without it, a subscriber that logged nothing would pass.
        assert!(
            logged.contains("/api/health"),
            "the path is missing from: {logged}"
        );
        assert!(
            !logged.contains(SECRET),
            "the token reached the log: {logged}"
        );
    }

    #[test]
    fn no_header_reaches_the_log() {
        // `Referer` on this site is a page the user was about to screenshot, and `Authorization`
        // is the credential itself. Neither is worth an access log line.
        let req = Request::builder()
            .uri("/api/health")
            .header("referer", format!("https://vriltrainer.de/#t={SECRET}"))
            .header("authorization", format!("Bearer {SECRET}"))
            .body(Body::empty())
            .expect("a valid request");
        let (_, logged) = serve(req);

        assert!(
            !logged.contains(SECRET),
            "a header reached the log: {logged}"
        );
        assert!(
            !logged.to_lowercase().contains("referer"),
            "referrers are logged: {logged}"
        );
    }

    /// FR-051: the line says which endpoint was called, never who it was called about. An account
    /// identifier in a path is the case D28 names, and it is the case a raw-path access log would
    /// write down on every review click.
    #[test]
    fn the_pattern_is_logged_and_the_identifier_in_the_path_is_not() {
        let req = Request::builder()
            .uri("/api/thing/7F3A9C")
            .body(Body::empty())
            .unwrap();
        let (response, logged) = serve(req);

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            logged.contains("/api/thing/{id}"),
            "the matched pattern is missing from: {logged}"
        );
        assert!(
            !logged.contains("7F3A9C"),
            "the account identifier reached the log: {logged}"
        );
    }

    /// A path the router did not match is a static asset or a client route, and one of those is
    /// somebody's shared proof link. nginx has the addresses; this log has the shape of the
    /// traffic.
    #[test]
    fn an_unmatched_path_is_not_written_down() {
        let req = Request::builder()
            .uri("/verify/7F3A9C")
            .body(Body::empty())
            .unwrap();
        let (_, logged) = serve(req);
        assert!(
            !logged.contains("/verify/"),
            "an unmatched path reached the log: {logged}"
        );
    }

    /// Both processes write to one journal (D24). Without the locale a line cannot be attributed
    /// to the domain that produced it.
    #[test]
    fn every_line_names_the_locale() {
        let req = Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();
        let (_, logged) = serve(req);
        assert!(logged.contains("locale"), "no locale field in: {logged}");
        assert!(logged.contains("\"de\"") || logged.contains("locale=de"));
    }

    #[test]
    fn every_response_carries_an_identifier_that_was_logged() {
        let req = Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();
        let (response, logged) = serve(req);

        let id = response
            .headers()
            .get(REQUEST_ID)
            .expect("the response names the request")
            .to_str()
            .expect("the identifier is ascii")
            .to_owned();
        assert_eq!(id.len(), 16);
        assert!(
            logged.contains(&id),
            "the identifier {id} is not in: {logged}"
        );
    }

    #[test]
    fn a_usable_inbound_identifier_is_kept() {
        let req = Request::builder()
            .uri("/api/health")
            .header(REQUEST_ID, "nginx-0123456789abcdef")
            .body(Body::empty())
            .unwrap();
        let (response, _) = serve(req);
        assert_eq!(
            response.headers().get(REQUEST_ID).unwrap(),
            "nginx-0123456789abcdef"
        );
    }

    #[test]
    fn an_inbound_identifier_that_could_write_the_log_is_replaced() {
        // Spaces and `=` are how a caller would forge fields into the line that names it.
        let req = Request::builder()
            .uri("/api/health")
            .header(REQUEST_ID, "abc account=victim")
            .body(Body::empty())
            .unwrap();
        let (response, logged) = serve(req);

        assert_ne!(
            response.headers().get(REQUEST_ID).unwrap(),
            "abc account=victim"
        );
        assert!(
            !logged.contains("account=victim"),
            "the log was written by the caller: {logged}"
        );
    }
}
