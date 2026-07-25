//! The locale, which is a startup flag and not a request property (D24).
//!
//! D10 originally selected the language bundle from the `Host` header. That made a proxy
//! misconfiguration serve the wrong language instead of failing, so D24 replaced it with two
//! processes and a hard `--locale` switch. Nothing in this module may reintroduce a path from a
//! request to a language.

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

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
    todo!("T023: compare `host` against `locale.domain()`, ignoring port and case")
}
