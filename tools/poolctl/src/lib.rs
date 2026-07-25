//! poolctl — the image pool's build step and its curation interface (D17).
//!
//! The pool is the one part of this project no software can produce: the pipeline can be
//! automated, the selection cannot (`docs/curation-guide.md`). What is automated here is everything
//! that must be identical across images — [`normalise`] removes the differences that would let a
//! target be spotted without anything paranormal, and [`manifest`] fixes the ordering the
//! derivation indexes into. [`annotate`] holds what only the operator knows, [`spec`] is the
//! file the operator writes it in, and [`check`] refuses the mistakes that are only cheap to fix
//! before a version is cut.

pub mod annotate;
pub mod check;
pub mod manifest;
pub mod normalise;
pub mod spec;
