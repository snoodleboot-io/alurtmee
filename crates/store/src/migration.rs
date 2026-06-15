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
pub(crate) const SCHEMA_VERSION: i64 = 5;

/// Bring `conn` up to [`SCHEMA_VERSION`], applying only the steps not yet present.
///
/// Idempotent and safe to call on every open: it reads `user_version` and applies each missing
/// step in order, bumping `user_version` as it goes.
///
/// - v1 introduces `config(key, value)`, which backs non-secret configuration (e.g. the repo
///   selection JSON).
/// - v2 introduces `etags` (HTTP conditional-request validators) and `pull_requests` (the cached
///   open-PR snapshot per repo).
/// - v3 introduces `reviews`, `comments`, and `pr_tests` (the per-PR enrichment payload: submitted
///   reviews, merged comments, and the reconciled CI verdict). `idx` columns preserve list order.
/// - v4 introduces `corrections` (a per-PR user category override) and `repo_classifier_config`
///   (the per-repo label map and bot overrides backing the feature-vs-security classifier, AD-5).
/// - v5 introduces `ci_runs` (recorded workflow-run outcomes per repo, keyed by `(repo, run_id)`),
///   backing recent-duration lookups for completed runs.
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

    if version < 2 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS etags (
                 endpoint      TEXT PRIMARY KEY NOT NULL,
                 etag          TEXT,
                 last_modified TEXT
             );
             CREATE TABLE IF NOT EXISTS pull_requests (
                 repo        TEXT    NOT NULL,
                 number      INTEGER NOT NULL,
                 title       TEXT    NOT NULL,
                 author      TEXT    NOT NULL,
                 draft       INTEGER NOT NULL,
                 updated_at  TEXT    NOT NULL,
                 url         TEXT    NOT NULL,
                 head_sha    TEXT    NOT NULL DEFAULT '',
                 author_type TEXT    NOT NULL DEFAULT '',
                 head_ref    TEXT    NOT NULL DEFAULT '',
                 labels_json TEXT    NOT NULL DEFAULT '[]',
                 PRIMARY KEY (repo, number)
             );
             PRAGMA user_version = 2;",
        )?;
    }

    if version < 3 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS reviews (
                 repo         TEXT    NOT NULL,
                 number       INTEGER NOT NULL,
                 idx          INTEGER NOT NULL,
                 author       TEXT    NOT NULL,
                 state        TEXT    NOT NULL,
                 submitted_at TEXT    NOT NULL,
                 PRIMARY KEY (repo, number, idx)
             );
             CREATE TABLE IF NOT EXISTS comments (
                 repo       TEXT    NOT NULL,
                 number     INTEGER NOT NULL,
                 idx        INTEGER NOT NULL,
                 kind       TEXT    NOT NULL,
                 author     TEXT    NOT NULL,
                 body       TEXT    NOT NULL,
                 created_at TEXT    NOT NULL,
                 PRIMARY KEY (repo, number, idx)
             );
             CREATE TABLE IF NOT EXISTS pr_tests (
                 repo    TEXT    NOT NULL,
                 number  INTEGER NOT NULL,
                 passed  INTEGER NOT NULL,
                 failed  INTEGER NOT NULL,
                 pending INTEGER NOT NULL,
                 state   TEXT    NOT NULL,
                 PRIMARY KEY (repo, number)
             );
             PRAGMA user_version = 3;",
        )?;
    }

    if version < 4 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS corrections (
                 repo     TEXT    NOT NULL,
                 number   INTEGER NOT NULL,
                 category TEXT    NOT NULL,
                 PRIMARY KEY (repo, number)
             );
             CREATE TABLE IF NOT EXISTS repo_classifier_config (
                 repo               TEXT PRIMARY KEY NOT NULL,
                 label_map_json     TEXT NOT NULL DEFAULT '{}',
                 bot_overrides_json TEXT NOT NULL DEFAULT '{}'
             );
             PRAGMA user_version = 4;",
        )?;
    }

    if version < SCHEMA_VERSION {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ci_runs (
                 repo          TEXT    NOT NULL,
                 run_id        INTEGER NOT NULL,
                 workflow      TEXT    NOT NULL,
                 conclusion    TEXT,
                 duration_secs INTEGER NOT NULL,
                 PRIMARY KEY (repo, run_id)
             );
             PRAGMA user_version = 5;",
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
    fn fresh_migrate_creates_all_tables_at_current_version() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        migrate(&conn).expect("migrate");

        assert!(table_exists(&conn, "config"));
        assert!(table_exists(&conn, "etags"));
        assert!(table_exists(&conn, "pull_requests"));
        assert!(table_exists(&conn, "reviews"));
        assert!(table_exists(&conn, "comments"));
        assert!(table_exists(&conn, "pr_tests"));
        assert!(table_exists(&conn, "corrections"));
        assert!(table_exists(&conn, "repo_classifier_config"));
        assert!(table_exists(&conn, "ci_runs"));
        assert_eq!(user_version(&conn), 5);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        migrate(&conn).expect("first migrate");
        migrate(&conn).expect("second migrate");
        migrate(&conn).expect("third migrate");

        assert_eq!(user_version(&conn), 5);
        assert!(table_exists(&conn, "config"));
        assert!(table_exists(&conn, "etags"));
        assert!(table_exists(&conn, "pull_requests"));
        assert!(table_exists(&conn, "reviews"));
        assert!(table_exists(&conn, "comments"));
        assert!(table_exists(&conn, "pr_tests"));
        assert!(table_exists(&conn, "corrections"));
        assert!(table_exists(&conn, "repo_classifier_config"));
        assert!(table_exists(&conn, "ci_runs"));
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

        migrate(&conn).expect("upgrade v1 -> current");

        // Version advanced and the new tables landed.
        assert_eq!(user_version(&conn), 5);
        assert!(table_exists(&conn, "etags"));
        assert!(table_exists(&conn, "pull_requests"));
        assert!(table_exists(&conn, "reviews"));
        assert!(table_exists(&conn, "comments"));
        assert!(table_exists(&conn, "pr_tests"));
        assert!(table_exists(&conn, "corrections"));
        assert!(table_exists(&conn, "repo_classifier_config"));
        assert!(table_exists(&conn, "ci_runs"));

        // The pre-existing v1 config row survived — the upgrade is non-destructive.
        let value: String = conn
            .query_row("SELECT value FROM config WHERE key = 'theme'", [], |row| {
                row.get(0)
            })
            .expect("v1 config row survives");
        assert_eq!(value, "dark");
    }

    #[test]
    fn incremental_upgrade_from_v2_preserves_data_and_adds_v3_tables() {
        // Simulate a database written by the v2 build: config + etags + pull_requests exist and
        // user_version is pinned at 2.
        let conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(
            "CREATE TABLE config (
                 key   TEXT PRIMARY KEY NOT NULL,
                 value TEXT NOT NULL
             );
             CREATE TABLE etags (
                 endpoint      TEXT PRIMARY KEY NOT NULL,
                 etag          TEXT,
                 last_modified TEXT
             );
             CREATE TABLE pull_requests (
                 repo       TEXT    NOT NULL,
                 number     INTEGER NOT NULL,
                 title      TEXT    NOT NULL,
                 author     TEXT    NOT NULL,
                 draft      INTEGER NOT NULL,
                 updated_at TEXT    NOT NULL,
                 url        TEXT    NOT NULL,
                 head_sha   TEXT    NOT NULL DEFAULT '',
                 PRIMARY KEY (repo, number)
             );
             PRAGMA user_version = 2;",
        )
        .expect("seed v2 schema");
        conn.execute(
            "INSERT INTO pull_requests
                 (repo, number, title, author, draft, updated_at, url, head_sha)
             VALUES ('octocat/hello', 7, 'A PR', 'octocat', 0, '2026-06-14T00:00:00Z',
                     'https://github.com/octocat/hello/pull/7', 'abc')",
            [],
        )
        .expect("seed v2 pull_requests row");

        assert_eq!(user_version(&conn), 2);

        migrate(&conn).expect("upgrade v2 -> current");

        // Version advanced and the three v3 enrichment tables landed.
        assert!(table_exists(&conn, "reviews"));
        assert!(table_exists(&conn, "comments"));
        assert!(table_exists(&conn, "pr_tests"));

        // The pre-existing v2 pull_requests row survived — the upgrade is non-destructive.
        let title: String = conn
            .query_row(
                "SELECT title FROM pull_requests WHERE repo = 'octocat/hello' AND number = 7",
                [],
                |row| row.get(0),
            )
            .expect("v2 pull_requests row survives");
        assert_eq!(title, "A PR");
    }

    #[test]
    fn incremental_upgrade_from_v3_preserves_data_and_adds_v4_tables() {
        // Simulate a database written by the v3 build: the enrichment tables exist (we seed just
        // pr_tests here) and user_version is pinned at 3.
        let conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(
            "CREATE TABLE pr_tests (
                 repo    TEXT    NOT NULL,
                 number  INTEGER NOT NULL,
                 passed  INTEGER NOT NULL,
                 failed  INTEGER NOT NULL,
                 pending INTEGER NOT NULL,
                 state   TEXT    NOT NULL,
                 PRIMARY KEY (repo, number)
             );
             PRAGMA user_version = 3;",
        )
        .expect("seed v3 schema");
        conn.execute(
            "INSERT INTO pr_tests (repo, number, passed, failed, pending, state)
             VALUES ('octocat/hello', 7, 3, 0, 0, 'passing')",
            [],
        )
        .expect("seed v3 pr_tests row");

        assert_eq!(user_version(&conn), 3);

        migrate(&conn).expect("upgrade v3 -> current");

        // Version advanced to current and the two v4 classifier-config tables landed.
        assert_eq!(user_version(&conn), 5);
        assert!(table_exists(&conn, "corrections"));
        assert!(table_exists(&conn, "repo_classifier_config"));

        // The pre-existing v3 pr_tests row survived — the upgrade is non-destructive.
        let passed: i64 = conn
            .query_row(
                "SELECT passed FROM pr_tests WHERE repo = 'octocat/hello' AND number = 7",
                [],
                |row| row.get(0),
            )
            .expect("v3 pr_tests row survives");
        assert_eq!(passed, 3);
    }

    #[test]
    fn incremental_upgrade_from_v4_preserves_data_and_adds_ci_runs() {
        // Simulate a database written by the v4 build: the classifier-config tables exist (we seed
        // just corrections here) and user_version is pinned at 4.
        let conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(
            "CREATE TABLE corrections (
                 repo     TEXT    NOT NULL,
                 number   INTEGER NOT NULL,
                 category TEXT    NOT NULL,
                 PRIMARY KEY (repo, number)
             );
             PRAGMA user_version = 4;",
        )
        .expect("seed v4 schema");
        conn.execute(
            "INSERT INTO corrections (repo, number, category)
             VALUES ('octocat/hello', 7, '\"feature\"')",
            [],
        )
        .expect("seed v4 corrections row");

        assert_eq!(user_version(&conn), 4);

        migrate(&conn).expect("upgrade v4 -> v5");

        // Version advanced and the v5 ci_runs table landed.
        assert_eq!(user_version(&conn), 5);
        assert!(table_exists(&conn, "ci_runs"));

        // The pre-existing v4 corrections row survived — the upgrade is non-destructive.
        let category: String = conn
            .query_row(
                "SELECT category FROM corrections WHERE repo = 'octocat/hello' AND number = 7",
                [],
                |row| row.get(0),
            )
            .expect("v4 corrections row survives");
        assert_eq!(category, "\"feature\"");
    }
}
