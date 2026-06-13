//! `gh-client` — GitHub API access for Alurtmee.
//!
//! Owns auth, REST + conditional (ETag/If-None-Match) requests, rate-limit / `X-Poll-Interval`
//! handling, and optional GraphQL enrichment (ARD AD-3/AD-4). Phase 0 establishes the crate seam
//! and error type; the HTTP client (`reqwest`) and request logic are introduced in Phase 1.

// `wiremock` is wired as a dev-dependency for Phase 1 HTTP contract tests (no HTTP client yet).

mod client;
mod error;

pub use client::GhClient;
pub use error::GhError;
