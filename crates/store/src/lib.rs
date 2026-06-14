//! `store` — local persistence for Alurtmee.
//!
//! Bundled SQLite (`rusqlite`) backs non-secret configuration — notably the repo selection,
//! round-tripped as JSON through the `config` table. The GitHub PAT lives in the OS keychain
//! ([`Keychain`]), never in SQLite, config files, or logs (ARD AD-6, SECURITY-critical). Schema is
//! versioned via SQLite `PRAGMA user_version` and migrated on every open.

mod error;
mod keychain;
mod migration;
mod store;

pub use error::StoreError;
pub use keychain::Keychain;
pub use store::Store;
