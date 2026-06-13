//! `store` — local persistence for Alurtmee.
//!
//! Bundled SQLite (`rusqlite`) backs the cache, ETags, CI baselines, and config; the PAT lives in
//! the OS keychain, never here (ARD AD-6). Phase 0 wires the dependency and a connection seam so
//! later phases add the schema and queries against a type that already opens a real database.

mod error;
mod store;

pub use error::StoreError;
pub use store::Store;
