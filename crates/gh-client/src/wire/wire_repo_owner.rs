use serde::Deserialize;

/// The nested `owner` object inside a GitHub repository payload. We keep the login and the owner
/// `type` (`"User"` or `"Organization"`), which tells us authoritatively whether the repo is
/// org-owned — flattened into `domain::Repo`.
#[derive(Debug, Deserialize)]
pub(crate) struct WireRepoOwner {
    pub login: String,
    /// GitHub's account type: `"Organization"` for org-owned repos, otherwise `"User"`.
    #[serde(rename = "type", default)]
    pub kind: String,
}
