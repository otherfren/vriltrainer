//! Which address a request is attributed to, behind the shared nginx (R8, D17).

// The parameter names below are the contract with whoever implements these; the `todo!()`
// bodies do not use them yet. Delete this line with the last `todo!()`.
#![allow(unused_variables)]

use std::net::IpAddr;

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
    todo!("T024: honour the forwarded address only when `peer` is in `trusted`")
}
