use rusqlite::Connection;

use crate::error::StoreError;

/// Owns the SQLite connection for the application.
///
/// Phase 0 provides real open paths (file-backed and in-memory) so the connection seam is
/// exercised end-to-end; schema creation and queries attach in later phases.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open a file-backed database at `path`, creating it if absent.
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    /// Open an ephemeral in-memory database (used by tests and one-shot tooling).
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        Ok(Self { conn })
    }

    /// Borrow the underlying connection for schema/query work in later phases.
    pub fn connection(&self) -> &Connection {
        &self.conn
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
}
