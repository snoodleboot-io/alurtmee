//! Schema migrations, versioned via SQLite's `PRAGMA user_version`.
//!
//! `user_version` is a plain integer SQLite stores in the database header, so it costs nothing and
//! needs no bookkeeping table. Each migration is gated on the current version and bumps it, which
//! makes `migrate` idempotent: re-opening an up-to-date database is a no-op.

use rusqlite::Connection;

use crate::error::StoreError;

/// The schema version this build expects after a successful migration.
pub(crate) const SCHEMA_VERSION: i64 = 1;

/// Bring `conn` up to [`SCHEMA_VERSION`], applying only the steps not yet present.
///
/// Idempotent and safe to call on every open: it reads `user_version` and applies the gap. v1
/// introduces the `config(key, value)` table that backs non-secret configuration (e.g. the repo
/// selection JSON). Secrets never land here — the GitHub token lives in the OS keychain only.
pub(crate) fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    if version < SCHEMA_VERSION {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS config (
                 key   TEXT PRIMARY KEY NOT NULL,
                 value TEXT NOT NULL
             );
             PRAGMA user_version = 1;",
        )?;
    }

    Ok(())
}
