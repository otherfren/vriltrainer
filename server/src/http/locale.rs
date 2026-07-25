//! The locale, which is a startup flag and not a request property (D24).
//!
//! D10 originally selected the language bundle from the `Host` header. That made a proxy
//! misconfiguration serve the wrong language instead of failing, so D24 replaced it with two
//! processes and a hard `--locale` switch. Nothing in this module may reintroduce a path from a
//! request to a language.
//!
//! Handlers that need the locale read `state.config.locale`. There is deliberately no second
//! source: a request extension holding a copy would be one more thing that could disagree with the
//! flag the process was started with.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, header};
use axum::middleware::Next;

use crate::config::Locale;

/// The `Content-Language` this process always sends.
pub fn content_language(locale: Locale) -> &'static str {
    locale.code()
}

/// Whether the request's `Host` is the domain this process is supposed to serve.
///
/// Nothing depends on the answer — the locale is already decided. It exists so a misrouted proxy
/// shows up as a warning in the log rather than as a site quietly serving German to `.com`.
pub fn host_matches(locale: Locale, host: Option<&str>) -> bool {
    let Some(host) = host else { return false };
    let host = host.trim();

    // Strip a port, but only a real one: an IPv6 literal is full of colons and its last group is
    // digits often enough that "everything after the last colon" would eat half the address.
    let name = match host.rsplit_once(':') {
        Some((name, port))
            if !port.is_empty()
                && port.bytes().all(|b| b.is_ascii_digit())
                && !name.ends_with(']') =>
        {
            name
        }
        Some(_) | None => host,
    };

    // `www.` is the same site to nginx, so treating it as a mismatch would produce a warning that
    // means nothing and trains the operator to ignore this line.
    let bare = match name.split_once('.') {
        Some((first, rest)) if first.eq_ignore_ascii_case("www") => rest,
        _ => name,
    };
    bare.eq_ignore_ascii_case(locale.domain())
}

/// Declares the locale on every response, and reports a `Host` that does not belong to this
/// process.
///
/// The `Content-Language` header is the only thing here that a client acts on. The warning is for
/// the operator: two processes serve two domains, so a proxy that sends `.com` traffic to the
/// German process would otherwise be visible only as users complaining about the language.
pub fn announce(router: Router, locale: Locale) -> Router {
    // A misrouted proxy is a standing condition, not an event. Warning per request would bury the
    // log line in the log it is meant to be noticed in.
    let warned = Arc::new(AtomicBool::new(false));
    let value = HeaderValue::from_static(content_language(locale));

    router.layer(axum::middleware::from_fn(move |req: Request, next: Next| {
        let warned = Arc::clone(&warned);
        let value = value.clone();
        async move {
            let host = req.headers().get(header::HOST).and_then(|h| h.to_str().ok());
            if !host_matches(locale, host) && !warned.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    host = host.unwrap_or("-"),
                    expected = locale.domain(),
                    "Host is not this process's domain; the locale is the --locale flag regardless"
                );
            }
            let mut response = next.run(req).await;
            response.headers_mut().insert(header::CONTENT_LANGUAGE, value);
            response
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_domain_matches_whatever_the_case_and_port() {
        assert!(host_matches(Locale::De, Some("vriltrainer.de")));
        assert!(host_matches(Locale::De, Some("VrilTrainer.DE")));
        assert!(host_matches(Locale::De, Some("vriltrainer.de:8080")));
        assert!(host_matches(Locale::De, Some("www.vriltrainer.de")));
        assert!(host_matches(Locale::En, Some("vriltrainer.com")));
    }

    #[test]
    fn the_other_domain_does_not_match() {
        // The whole point of the warning: this is the request that used to be served in the wrong
        // language without anything going wrong.
        assert!(!host_matches(Locale::De, Some("vriltrainer.com")));
        assert!(!host_matches(Locale::En, Some("vriltrainer.de")));
    }

    #[test]
    fn absent_or_foreign_hosts_do_not_match() {
        assert!(!host_matches(Locale::De, None));
        assert!(!host_matches(Locale::De, Some("")));
        assert!(!host_matches(Locale::De, Some("evil.example")));
        assert!(!host_matches(Locale::De, Some("vriltrainer.de.evil.example")));
        assert!(!host_matches(Locale::De, Some("[2001:db8::1]:8080")));
    }
}
