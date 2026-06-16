//! Classifier persistence (AD-5): per-PR user category corrections and per-repo classifier config
//! (the label map and bot overrides backing the feature-vs-security classifier).

use rusqlite::{params, OptionalExtension};

use crate::error::StoreError;
use crate::store::Store;

impl Store {
    /// Insert or overwrite the user's per-PR category override.
    ///
    /// The category is stored as text via `serde_json`, which serializes a [`domain::CategoryKind`]
    /// to a bare quoted string (`"feature"`/`"security"`/`"unknown"`); [`get_correction`] parses it
    /// back symmetrically. Only this enum value is written — never a token (ARD AD-6).
    ///
    /// [`get_correction`]: Self::get_correction
    pub fn set_correction(
        &self,
        repo: &str,
        number: u64,
        kind: domain::CategoryKind,
    ) -> Result<(), StoreError> {
        let category =
            serde_json::to_string(&kind).map_err(|e| StoreError::Decode(e.to_string()))?;
        self.conn.execute(
            "INSERT INTO corrections (repo, number, category) VALUES (?1, ?2, ?3)
             ON CONFLICT(repo, number) DO UPDATE SET category = excluded.category",
            params![repo, number, category],
        )?;
        Ok(())
    }

    /// Read the user's category override for a PR, returning `None` if none was set.
    ///
    /// An unparseable stored value maps to [`StoreError::Decode`].
    pub fn get_correction(
        &self,
        repo: &str,
        number: u64,
    ) -> Result<Option<domain::CategoryKind>, StoreError> {
        let category: Option<String> = self
            .conn
            .query_row(
                "SELECT category FROM corrections WHERE repo = ?1 AND number = ?2",
                params![repo, number],
                |row| row.get(0),
            )
            .optional()?;
        match category {
            None => Ok(None),
            Some(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| StoreError::Decode(e.to_string())),
        }
    }

    /// Remove the user's category override for a PR.
    ///
    /// Deleting an absent row is a no-op and returns `Ok`.
    pub fn clear_correction(&self, repo: &str, number: u64) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM corrections WHERE repo = ?1 AND number = ?2",
            params![repo, number],
        )?;
        Ok(())
    }

    /// Insert or overwrite the per-repo label map.
    ///
    /// Touches only `label_map_json`, leaving any stored bot overrides for the same repo intact.
    pub fn save_label_map(&self, repo: &str, map: &domain::LabelMap) -> Result<(), StoreError> {
        let json = serde_json::to_string(map).map_err(|e| StoreError::Decode(e.to_string()))?;
        self.conn.execute(
            "INSERT INTO repo_classifier_config (repo, label_map_json) VALUES (?1, ?2)
             ON CONFLICT(repo) DO UPDATE SET label_map_json = excluded.label_map_json",
            params![repo, json],
        )?;
        Ok(())
    }

    /// Load the per-repo label map, returning `None` if the repo has no config row.
    ///
    /// Malformed stored JSON maps to [`StoreError::Decode`].
    pub fn load_label_map(&self, repo: &str) -> Result<Option<domain::LabelMap>, StoreError> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT label_map_json FROM repo_classifier_config WHERE repo = ?1",
                params![repo],
                |row| row.get(0),
            )
            .optional()?;
        match json {
            None => Ok(None),
            Some(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| StoreError::Decode(e.to_string())),
        }
    }

    /// Insert or overwrite the per-repo bot overrides.
    ///
    /// Touches only `bot_overrides_json`, leaving any stored label map for the same repo intact.
    pub fn save_bot_overrides(
        &self,
        repo: &str,
        overrides: &domain::BotOverrides,
    ) -> Result<(), StoreError> {
        let json =
            serde_json::to_string(overrides).map_err(|e| StoreError::Decode(e.to_string()))?;
        self.conn.execute(
            "INSERT INTO repo_classifier_config (repo, bot_overrides_json) VALUES (?1, ?2)
             ON CONFLICT(repo) DO UPDATE SET bot_overrides_json = excluded.bot_overrides_json",
            params![repo, json],
        )?;
        Ok(())
    }

    /// Load the per-repo bot overrides, returning `None` if the repo has no config row.
    ///
    /// Malformed stored JSON maps to [`StoreError::Decode`].
    pub fn load_bot_overrides(
        &self,
        repo: &str,
    ) -> Result<Option<domain::BotOverrides>, StoreError> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT bot_overrides_json FROM repo_classifier_config WHERE repo = ?1",
                params![repo],
                |row| row.get(0),
            )
            .optional()?;
        match json {
            None => Ok(None),
            Some(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| StoreError::Decode(e.to_string())),
        }
    }
}
