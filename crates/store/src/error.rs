/// Errors surfaced by the persistence layer.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A SQLite operation failed (open, migration, or query).
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// An OS keychain operation failed (set/get/delete of the GitHub token).
    #[error("keychain error: {0}")]
    Keyring(#[from] keyring::Error),

    /// A persisted value could not be decoded (e.g. malformed `repo_selection` JSON).
    #[error("decode error: {0}")]
    Decode(String),
}
