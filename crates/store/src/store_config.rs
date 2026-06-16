//! Non-secret key/value configuration and the persisted repo selection.

use rusqlite::{params, OptionalExtension};

use crate::error::StoreError;
use crate::store::Store;

/// Config key under which the persisted [`domain::RepoSelection`] JSON is stored.
const REPO_SELECTION_KEY: &str = "repo_selection";

impl Store {
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
