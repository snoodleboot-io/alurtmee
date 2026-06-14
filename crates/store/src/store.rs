use rusqlite::{params, Connection, OptionalExtension};

use crate::error::StoreError;
use crate::migration;

/// Config key under which the persisted [`domain::RepoSelection`] JSON is stored.
const REPO_SELECTION_KEY: &str = "repo_selection";

/// Owns the SQLite connection for the application.
///
/// Backs non-secret configuration only — notably the repo selection. The GitHub token never
/// touches this store; it lives in the OS keychain (see [`crate::Keychain`], ARD AD-6).
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open a file-backed database at `path`, creating it if absent, and run migrations.
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let store = Self {
            conn: Connection::open(path)?,
        };
        migration::migrate(&store.conn)?;
        Ok(store)
    }

    /// Open an ephemeral in-memory database (used by tests and one-shot tooling) and migrate it.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let store = Self {
            conn: Connection::open_in_memory()?,
        };
        migration::migrate(&store.conn)?;
        Ok(store)
    }

    /// Borrow the underlying connection for ad-hoc schema/query work.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Insert or overwrite a non-secret config value.
    pub fn set_config(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO config (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Read a config value, returning `None` if the key is absent.
    pub fn get_config(&self, key: &str) -> Result<Option<String>, StoreError> {
        let value = self
            .conn
            .query_row(
                "SELECT value FROM config WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(value)
    }

    /// Persist the repo selection as JSON under the `repo_selection` config key.
    pub fn save_selection(&self, selection: &domain::RepoSelection) -> Result<(), StoreError> {
        let json =
            serde_json::to_string(selection).map_err(|e| StoreError::Decode(e.to_string()))?;
        self.set_config(REPO_SELECTION_KEY, &json)
    }

    /// Load the repo selection, returning the empty default if none has been saved.
    pub fn load_selection(&self) -> Result<domain::RepoSelection, StoreError> {
        match self.get_config(REPO_SELECTION_KEY)? {
            None => Ok(domain::RepoSelection::default()),
            Some(json) => {
                serde_json::from_str(&json).map_err(|e| StoreError::Decode(e.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_executes_trivial_query() {
        let store = Store::open_in_memory().expect("open in-memory database");
        let value: i64 = store
            .connection()
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("run SELECT 1");
        assert_eq!(value, 1);
    }

    #[test]
    fn open_creates_file_backed_database() {
        let mut path = std::env::temp_dir();
        path.push(format!("alurtmee_store_test_{}.sqlite", std::process::id()));
        let path_str = path.to_str().expect("utf-8 temp path");

        let store = Store::open(path_str).expect("open file-backed database");
        let value: i64 = store
            .connection()
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("run SELECT 1");
        assert_eq!(value, 1);

        drop(store);
        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn migration_creates_config_table_and_sets_version() {
        let store = Store::open_in_memory().expect("open + migrate");

        let table: Option<String> = store
            .connection()
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'config'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query sqlite_master");
        assert_eq!(table.as_deref(), Some("config"));

        let version: i64 = store
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, migration::SCHEMA_VERSION);
    }

    #[test]
    fn migration_is_idempotent() {
        let store = Store::open_in_memory().expect("open + migrate");

        // Re-run migration directly; version must stay at v1 and the table must survive.
        migration::migrate(store.connection()).expect("second migrate");
        migration::migrate(store.connection()).expect("third migrate");

        let version: i64 = store
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, migration::SCHEMA_VERSION);
    }

    #[test]
    fn config_round_trip_and_absent_key() {
        let store = Store::open_in_memory().expect("open store");

        assert_eq!(store.get_config("missing").expect("get absent"), None);

        store.set_config("theme", "dark").expect("set");
        assert_eq!(
            store.get_config("theme").expect("get present"),
            Some("dark".to_string())
        );

        // UPSERT overwrites.
        store.set_config("theme", "light").expect("update");
        assert_eq!(
            store.get_config("theme").expect("get updated"),
            Some("light".to_string())
        );
    }

    #[test]
    fn selection_round_trip() {
        let store = Store::open_in_memory().expect("open store");

        // Fresh store yields the empty default.
        assert!(store.load_selection().expect("load default").is_empty());

        let selection: domain::RepoSelection = ["octocat/hello", "rust-lang/rust"]
            .into_iter()
            .map(String::from)
            .collect();
        store.save_selection(&selection).expect("save");

        let loaded = store.load_selection().expect("load saved");
        assert_eq!(loaded, selection);
    }

    #[test]
    fn load_selection_rejects_malformed_json() {
        let store = Store::open_in_memory().expect("open store");
        store
            .set_config("repo_selection", "{ not valid json")
            .expect("seed bad json");

        let err = store.load_selection().expect_err("decode should fail");
        assert!(matches!(err, StoreError::Decode(_)));
    }
}
