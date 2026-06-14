//! Schema migrations, versioned via SQLite's `PRAGMA user_version`.
//!
//! `user_version` is a plain integer SQLite stores in the database header, so it costs nothing and
//! needs no bookkeeping table. Each migration is gated on the current version and bumps it, which
//! makes `migrate` idempotent: re-opening an up-to-date database is a no-op. Steps are applied
//! incrementally (v0→v1→v2…), so a database created by an older build upgrades in place without
//! dropping the data earlier steps wrote.

use rusqlite::Connection;

use crate::error::StoreError;

/// The schema version this build expects after a successful migration.
pub(crate) const SCHEMA_VERSION: i64 = 2;

/// Bring `conn` up to [`SCHEMA_VERSION`], applying only the steps not yet present.
///
/// Idempotent and safe to call on every open: it reads `user_version` and applies each missing
/// step in order, bumping `user_version` as it goes.
///
/// - v1 introduces `config(key, value)`, which backs non-secret configuration (e.g. the repo
///   selection JSON).
/// - v2 introduces `etags` (HTTP conditional-request validators) and `pull_requests` (the cached
///   open-PR snapshot per repo).
///
/// Secrets never land in any of these tables — the GitHub token lives in the OS keychain only
/// (ARD AD-6).
pub(crate) fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    if version < 1 {
        // `user_version` cannot be a bound parameter, so the integer is inlined.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS config (
                 key   TEXT PRIMARY KEY NOT NULL,
                 value TEXT NOT NULL
             );
             PRAGMA user_version = 1;",
        )?;
    }

    if version < SCHEMA_VERSION {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS etags (
                 endpoint      TEXT PRIMARY KEY NOT NULL,
                 etag          TEXT,
                 last_modified TEXT
             );
             CREATE TABLE IF NOT EXISTS pull_requests (
                 repo       TEXT    NOT NULL,
                 number     INTEGER NOT NULL,
                 title      TEXT    NOT NULL,
                 author     TEXT    NOT NULL,
                 draft      INTEGER NOT NULL,
                 updated_at TEXT    NOT NULL,
                 url        TEXT    NOT NULL,
                 PRIMARY KEY (repo, number)
             );
             PRAGMA user_version = 2;",
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_exists(conn: &Connection, name: &str) -> bool {
        use rusqlite::OptionalExtension;
        conn.query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .expect("query sqlite_master")
        .is_some()
    }

    fn user_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version")
    }

    #[test]
    fn fresh_migrate_creates_all_v2_tables_at_version_2() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        migrate(&conn).expect("migrate");

        assert!(table_exists(&conn, "config"));
        assert!(table_exists(&conn, "etags"));
        assert!(table_exists(&conn, "pull_requests"));
        assert_eq!(user_version(&conn), 2);
    }

    #[test]
    fn migrate_is_idempotent_at_v2() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        migrate(&conn).expect("first migrate");
        migrate(&conn).expect("second migrate");
        migrate(&conn).expect("third migrate");

        assert_eq!(user_version(&conn), 2);
        assert!(table_exists(&conn, "config"));
        assert!(table_exists(&conn, "etags"));
        assert!(table_exists(&conn, "pull_requests"));
    }

    #[test]
    fn incremental_upgrade_from_v1_preserves_config_and_adds_v2_tables() {
        // Simulate a database written by the v1 build: only the `config` table exists and
        // user_version is pinned at 1.
        let conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(
            "CREATE TABLE config (
                 key   TEXT PRIMARY KEY NOT NULL,
                 value TEXT NOT NULL
             );
             PRAGMA user_version = 1;",
        )
        .expect("seed v1 schema");
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('theme', 'dark')",
            [],
        )
        .expect("seed v1 config row");

        assert_eq!(user_version(&conn), 1);

        migrate(&conn).expect("upgrade v1 -> v2");

        // Version advanced and the new tables landed.
        assert_eq!(user_version(&conn), 2);
        assert!(table_exists(&conn, "etags"));
        assert!(table_exists(&conn, "pull_requests"));

        // The pre-existing v1 config row survived — the upgrade is non-destructive.
        let value: String = conn
            .query_row("SELECT value FROM config WHERE key = 'theme'", [], |row| {
                row.get(0)
            })
            .expect("v1 config row survives");
        assert_eq!(value, "dark");
    }
}
