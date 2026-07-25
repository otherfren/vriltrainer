//! Which address a request is attributed to, behind the shared nginx (R8, D17).

use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;

use super::AppState;

/// The header `deploy/nginx.conf` writes, via `$proxy_add_x_forwarded_for`.
pub const FORWARDED_FOR: &str = "x-forwarded-for";

/// The client's address.
///
/// The forwarded header is honoured **only** when the immediate peer is one of `trusted`. Both
/// halves of that are load-bearing and each fails in the opposite direction. Ignore the header and
/// every request appears to come from `127.0.0.1`, so the per-address limit on account creation is
/// either inert or throttles all users together. Believe it from any peer and a client sets its
/// own address, so the limit is gone again — this time invisibly.
///
/// `forwarded_for` is the raw `X-Forwarded-For` value, which is a list: the proxy appends, so the
/// entry that matters is the last one it wrote, not the first one a client can invent.
pub fn client_addr(peer: IpAddr, forwarded_for: Option<&str>, trusted: &[IpAddr]) -> IpAddr {
    if !trusted.contains(&peer) {
        // Either nothing is proxying, or someone reached the port directly. Both are answered by
        // the one address that is not hearsay.
        return peer;
    }
    let Some(list) = forwarded_for else {
        return peer;
    };

    for entry in list.rsplit(',') {
        let Some(addr) = parse_entry(entry) else {
            // The rightmost entry is the one the proxy appended, so an unparsable one means the
            // proxy is misconfigured. Walking further left would start reading the part of the
            // list a client wrote, which is the forgery this function exists to refuse.
            break;
        };
        if !trusted.contains(&addr) {
            return addr;
        }
        // A trusted hop that another trusted hop appended: keep walking left through our own
        // infrastructure, and never past the first address it vouched for.
    }
    peer
}

/// One `X-Forwarded-For` element. Some proxies append a port, and `[::1]:8080` and `10.0.0.1:8080`
/// both occur.
fn parse_entry(entry: &str) -> Option<IpAddr> {
    let entry = entry.trim();
    entry
        .parse::<IpAddr>()
        .ok()
        .or_else(|| entry.parse::<SocketAddr>().ok().map(|s| s.ip()))
}

/// The address this request is attributed to — what the per-address creation limit counts against
/// (D17, T075).
#[derive(Debug, Clone, Copy)]
pub struct ClientAddr(pub IpAddr);

impl FromRequestParts<AppState> for ClientAddr {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Infallible> {
        let forwarded = parts
            .headers
            .get(FORWARDED_FOR)
            .and_then(|v| v.to_str().ok());
        let Some(ConnectInfo(peer)) = parts.extensions.get::<ConnectInfo<SocketAddr>>() else {
            // Reachable only when the service was built without connect info — see
            // [`super::service`]. Every request then falls into one bucket, so the limit throttles
            // all users together instead of letting anyone forge their way past it. Loud, because
            // the quiet version of this bug is the one R8 is about.
            warn_unconnected();
            return Ok(ClientAddr(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
        };
        Ok(ClientAddr(client_addr(
            peer.ip(),
            forwarded,
            &state.config.trusted_proxies,
        )))
    }
}

fn warn_unconnected() {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        tracing::error!(
            "no peer address on the request: the server was built without connect info, so every \
             client counts as one address"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    const PROXY: &str = "127.0.0.1";

    #[test]
    fn forwarded_address_is_honoured_from_the_proxy() {
        let trusted = [ip(PROXY)];
        assert_eq!(
            client_addr(ip(PROXY), Some("203.0.113.7"), &trusted),
            ip("203.0.113.7")
        );
    }

    #[test]
    fn forwarded_address_is_ignored_from_anyone_else() {
        // The forgery: something reaches the port directly and claims to be someone else, so the
        // creation limit would be spent on a stranger's address.
        let trusted = [ip(PROXY)];
        let peer = ip("198.51.100.9");
        assert_eq!(client_addr(peer, Some("203.0.113.7"), &trusted), peer);
    }

    #[test]
    fn an_empty_trust_list_never_honours_the_header() {
        assert_eq!(client_addr(ip(PROXY), Some("203.0.113.7"), &[]), ip(PROXY));
    }

    #[test]
    fn the_client_cannot_prepend_its_way_past_the_proxy() {
        // nginx appends what it saw, so anything a client wrote is to the left of it.
        let trusted = [ip(PROXY)];
        let forged = "203.0.113.7, 10.0.0.1, 198.51.100.9";
        assert_eq!(
            client_addr(ip(PROXY), Some(forged), &trusted),
            ip("198.51.100.9")
        );
    }

    #[test]
    fn trusted_hops_are_walked_past_and_client_entries_are_not() {
        // A trusted address in front of nginx: the rightmost entry is that hop, the one before it
        // is the address the hop vouched for.
        let trusted = [ip(PROXY), ip("10.0.0.8")];
        assert_eq!(
            client_addr(ip(PROXY), Some("203.0.113.7, 10.0.0.8"), &trusted),
            ip("203.0.113.7")
        );
    }

    #[test]
    fn a_junk_last_entry_falls_back_to_the_peer() {
        let trusted = [ip(PROXY)];
        assert_eq!(
            client_addr(ip(PROXY), Some("203.0.113.7, unknown"), &trusted),
            ip(PROXY)
        );
    }

    #[test]
    fn entries_may_carry_a_port() {
        let trusted = [ip(PROXY)];
        assert_eq!(
            client_addr(ip(PROXY), Some("203.0.113.7:44321"), &trusted),
            ip("203.0.113.7")
        );
        assert_eq!(
            client_addr(ip(PROXY), Some("[2001:db8::5]:443"), &trusted),
            ip("2001:db8::5")
        );
    }

    #[test]
    fn no_header_is_the_peer() {
        assert_eq!(client_addr(ip(PROXY), None, &[ip(PROXY)]), ip(PROXY));
    }

    /// The extractor over a real connection, because the rule above is only enforced if the peer
    /// actually reaches it — and it does so through `ConnectInfo`, which exists only when the
    /// service was built for it (see [`super::super::service`]).
    #[tokio::test(flavor = "multi_thread")]
    async fn the_peer_comes_from_the_connection() {
        use axum::Router;
        use axum::routing::get;

        use crate::http::test_support;

        let app = Router::new()
            .route(
                "/whoami",
                get(|ClientAddr(ip): ClientAddr| async move { ip.to_string() }),
            )
            .with_state(test_support::state())
            .into_make_service_with_connect_info::<SocketAddr>();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a free port");
        let addr = listener.local_addr().expect("the port is known");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("the server runs") });

        // The default trust list holds the loopback addresses, and the test connects from one, so
        // this stands in for the request arriving from nginx on the same host.
        let seen = get_whoami(addr, "X-Forwarded-For: 203.0.113.7\r\n").await;
        assert!(
            seen.ends_with("203.0.113.7"),
            "the proxy was not believed: {seen}"
        );

        let seen = get_whoami(addr, "").await;
        assert!(seen.ends_with("127.0.0.1"), "the peer was lost: {seen}");
    }

    /// One request, written by hand: the point of the test is the connection underneath it, which
    /// an in-process call to the router would not have.
    async fn get_whoami(addr: SocketAddr, extra_header: &str) -> String {
        use std::io::{Read, Write};

        let mut sock = std::net::TcpStream::connect(addr).expect("the server accepts");
        let req =
            format!("GET /whoami HTTP/1.1\r\nHost: t\r\nConnection: close\r\n{extra_header}\r\n");
        sock.write_all(req.as_bytes())
            .expect("the request is written");
        let mut response = String::new();
        sock.read_to_string(&mut response)
            .expect("the response is read");
        response
    }
}
