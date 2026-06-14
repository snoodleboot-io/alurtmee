//! `gh-client` — GitHub API access for Alurtmee.
//!
//! Owns auth, REST requests, pagination, and rate-limit handling (ARD AD-3/AD-4). Mock-first:
//! the base URL is injectable so tests drive the client against a `wiremock` server with no live
//! network and no real token. GitHub's JSON is mapped through a private wire/anti-corruption layer
//! ([`wire`]) so the pure `domain` types never depend on GitHub's schema quirks.

mod client;
mod error;
mod open_prs_result;
mod pr_outcome;
mod wire;

pub use client::GhClient;
pub use error::GhError;
pub use open_prs_result::OpenPrsResult;
pub use pr_outcome::PrOutcome;
