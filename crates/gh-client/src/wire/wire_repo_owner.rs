use serde::Deserialize;

/// The nested `owner` object inside a GitHub repository payload. Only the login is needed to
/// flatten into `domain::Repo`.
#[derive(Debug, Deserialize)]
pub(crate) struct WireRepoOwner {
    pub login: String,
}
