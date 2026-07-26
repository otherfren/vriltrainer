//! vriltrainer server.
//!
//! Design basis: `docs/trial-protocol-decisions.md` D1–D32, and the artifacts under
//! `specs/001-remote-viewing-trainer/`.
//!
//! Reading order for the parts that are load-bearing rather than merely present: [`trial::derive`]
//! is the derivation two implementations must agree on, [`log::chain`] owns the hashing rule for
//! the public record, and [`db::Db::append_with`] is what keeps two processes from forking it.

pub mod account;
pub mod config;
pub mod db;
pub mod framing;
pub mod http;
pub mod log;
pub mod pool;
pub mod stats;
pub mod trial;
pub mod vectors;
