//! Schema migrations, versioned via SQLite's `PRAGMA user_version`.
//!
//! `user_version` is a plain integer SQLite stores in the database header, so it costs nothing and
//! needs no bookkeeping table. Each migration is gated on the current version and bumps it, which
//! makes `migrate` idempotent: re-opening an up-to-date database is a no-op. Steps are applied
//! incrementally (v0→v1→v2…), so a database created by an older build upgrades in place without
//! dropping the data earlier steps wrote.

use rusqlite::Connection;

use crate::error::StoreError;

/// How a migration step is applied: either a batch of DDL, or Rust code for steps that need
/// conditional logic (e.g. adding a column only if an older, in-place-edited migration omitted it,
/// which plain `ALTER TABLE` cannot express idempotently).
enum Step {
    Sql(&'static str),
    Run(fn(&Connection) -> Result<(), StoreError>),
}

/// One schema step: how to apply it, and the `user_version` it advances the database to once done.
struct Migration {
    version: i64,
    apply: Step,
}

/// The ordered schema steps. **Adding a step is a single append here** — `migrate` applies whichever
/// are missing and stamps `user_version` for each, so a database from an older build upgrades in
/// place (v0→v1→v2…) without dropping the data earlier steps wrote, and re-opening an up-to-date
/// database is a no-op.
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
/// - v6 backfills the `pull_requests` columns (`head_sha`, `author_type`, `head_ref`, `labels_json`)
///   that early databases lack, repairing a schema that an in-place edit to the v2 step had left
///   inconsistent. A no-op on databases that already have them.
/// - v7 clears the `etags` cache so a database wedged by the v6-era inconsistency (recorded ETags
///   but empty PR cache → permanent 304s) refetches once and recovers.
///
/// Secrets never land in any of these tables — the GitHub token lives in the OS keychain only
/// (ARD AD-6).
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        apply: Step::Sql(
            "CREATE TABLE IF NOT EXISTS config (
                  key   TEXT PRIMARY KEY NOT NULL,
                  value TEXT NOT NULL
              );",
        ),
    },
    Migration {
        version: 2,
        apply: Step::Sql(
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
              );",
        ),
    },
    Migration {
        version: 3,
        apply: Step::Sql(
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
              );",
        ),
    },
    Migration {
        version: 4,
        apply: Step::Sql(
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
              );",
        ),
    },
    Migration {
        version: 5,
        apply: Step::Sql(
            "CREATE TABLE IF NOT EXISTS ci_runs (
                  repo          TEXT    NOT NULL,
                  run_id        INTEGER NOT NULL,
                  workflow      TEXT    NOT NULL,
                  conclusion    TEXT,
                  duration_secs INTEGER NOT NULL,
                  PRIMARY KEY (repo, run_id)
              );",
        ),
    },
    Migration {
        version: 6,
        // v6 backfills `pull_requests` columns (head_sha, author_type, head_ref, labels_json) onto
        // databases created before they existed. Early builds shipped a v2 `pull_requests` without
        // them; the columns were later added by editing the v2 DDL, so a database that had already
        // passed v2 never received them and every `load_repo_prs` failed. This step adds whichever
        // are missing — a no-op on fresh databases that already have them.
        apply: Step::Run(backfill_pull_request_columns),
    },
    Migration {
        version: 7,
        // Clear stale conditional-request validators. A database repaired by v6 had been failing to
        // cache PRs while still recording ETags, so every repo now 304s ("nothing changed") yet the
        // cache is empty — permanently wedged. Dropping the ETags forces one full refetch that
        // re-populates the cache. Harmless on a healthy database (it just refetches once).
        apply: Step::Run(clear_stale_etags),
    },
];

/// Empty the `etags` cache (if the table exists) so a wedged database refetches once and recovers.
fn clear_stale_etags(conn: &Connection) -> Result<(), StoreError> {
    let has_table: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'etags'",
        [],
        |row| row.get(0),
    )?;
    if has_table > 0 {
        conn.execute_batch("DELETE FROM etags;")?;
    }
    Ok(())
}

/// Add any `pull_requests` columns the current schema expects but an older database lacks.
fn backfill_pull_request_columns(conn: &Connection) -> Result<(), StoreError> {
    let existing: std::collections::HashSet<String> = conn
        .prepare("PRAGMA table_info(pull_requests)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;

    // An empty column set means the table does not exist (PRAGMA table_info yields no rows for a
    // missing table). There is nothing to backfill — v2 will have created it for any real database.
    if existing.is_empty() {
        return Ok(());
    }

    for (column, ddl) in [
        (
            "head_sha",
            "ALTER TABLE pull_requests ADD COLUMN head_sha TEXT NOT NULL DEFAULT ''",
        ),
        (
            "author_type",
            "ALTER TABLE pull_requests ADD COLUMN author_type TEXT NOT NULL DEFAULT ''",
        ),
        (
            "head_ref",
            "ALTER TABLE pull_requests ADD COLUMN head_ref TEXT NOT NULL DEFAULT ''",
        ),
        (
            "labels_json",
            "ALTER TABLE pull_requests ADD COLUMN labels_json TEXT NOT NULL DEFAULT '[]'",
        ),
    ] {
        if !existing.contains(column) {
            conn.execute_batch(ddl)?;
        }
    }
    Ok(())
}

/// The schema version this build expects after a successful migration — the last step's version.
pub(crate) const SCHEMA_VERSION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].version;

/// Bring `conn` up to [`SCHEMA_VERSION`] by applying each [`MIGRATIONS`] step whose version exceeds
/// the database's current `user_version`, stamping the version after each. Idempotent.
pub(crate) fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let mut version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    for step in MIGRATIONS {
        if version < step.version {
            match step.apply {
                Step::Sql(ddl) => conn.execute_batch(ddl)?,
                Step::Run(run) => run(conn)?,
            }
            // `user_version` cannot be a bound parameter, so the integer is inlined — it is a
            // trusted constant from MIGRATIONS, never user input.
            conn.execute_batch(&format!("PRAGMA user_version = {};", step.version))?;
            version = step.version;
        }
    }

    // Invariant: a migrated database is at the latest schema version (unless it was opened by a
    // newer build, in which case it is ahead — never behind).
    debug_assert!(
        version >= SCHEMA_VERSION,
        "migrations must leave the database at or above SCHEMA_VERSION"
    );

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
        assert_eq!(user_version(&conn), SCHEMA_VERSION);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        migrate(&conn).expect("first migrate");
        migrate(&conn).expect("second migrate");
        migrate(&conn).expect("third migrate");

        assert_eq!(user_version(&conn), SCHEMA_VERSION);
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
        assert_eq!(user_version(&conn), SCHEMA_VERSION);
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
        assert_eq!(user_version(&conn), SCHEMA_VERSION);
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
        assert_eq!(user_version(&conn), SCHEMA_VERSION);
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

    fn columns(conn: &Connection, table: &str) -> Vec<String> {
        conn.prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn v6_backfills_missing_pull_request_columns_on_a_legacy_db() {
        // Reproduce a database written by an early build: pull_requests has only its original 7
        // columns and user_version is pinned at 5 (the columns added by a later in-place edit to the
        // v2 DDL were never applied, since v2 had already run).
        let conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(
            "CREATE TABLE pull_requests (
                 repo       TEXT    NOT NULL,
                 number     INTEGER NOT NULL,
                 title      TEXT    NOT NULL,
                 author     TEXT    NOT NULL,
                 draft      INTEGER NOT NULL,
                 updated_at TEXT    NOT NULL,
                 url        TEXT    NOT NULL,
                 PRIMARY KEY (repo, number)
             );
             INSERT INTO pull_requests (repo, number, title, author, draft, updated_at, url)
             VALUES ('octocat/hello', 7, 'A PR', 'octocat', 0, 't', 'u');
             PRAGMA user_version = 5;",
        )
        .expect("seed legacy v5 schema");
        assert!(!columns(&conn, "pull_requests").contains(&"head_sha".to_string()));

        migrate(&conn).expect("upgrade v5 -> v6 backfill");

        let cols = columns(&conn, "pull_requests");
        for added in ["head_sha", "author_type", "head_ref", "labels_json"] {
            assert!(cols.contains(&added.to_string()), "v6 adds {added}");
        }
        assert_eq!(user_version(&conn), SCHEMA_VERSION);
        // The added columns took their defaults; the pre-existing row survived non-destructively.
        let (sha, labels): (String, String) = conn
            .query_row(
                "SELECT head_sha, labels_json FROM pull_requests WHERE number = 7",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("legacy row survives with defaulted columns");
        assert_eq!((sha.as_str(), labels.as_str()), ("", "[]"));
    }

    #[test]
    fn v6_is_a_noop_on_a_fresh_database() {
        // A database created by the current build already has the columns; v6 must not error.
        let conn = Connection::open_in_memory().expect("open in-memory");
        migrate(&conn).expect("fresh migrate");
        migrate(&conn).expect("re-migrate is idempotent");
        let cols = columns(&conn, "pull_requests");
        assert!(cols.contains(&"head_sha".to_string()));
    }
}
