use serde::{Deserialize, Serialize};

/// A GitHub repository the user may select for polling.
///
/// `full_name` (`owner/name`) is the stable identity we persist in [`RepoSelection`] and key
/// polling on; `owner`/`name` are kept split so callers don't re-parse the slug. `gh-client`
/// constructs this from GitHub's nested wire payload (where `owner` is an object) so this type
/// stays a flat, persistence-friendly value.
///
/// [`RepoSelection`]: crate::RepoSelection
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Repo {
    /// Stable numeric repository id.
    pub id: u64,
    /// Owner login (user or org) — the `{owner}` in `owner/name`.
    pub owner: String,
    /// Repository name — the `{name}` in `owner/name`.
    pub name: String,
    /// Canonical `owner/name` slug; the identity we persist and poll on.
    pub full_name: String,
    /// Whether the repository is private (affects required token scope).
    pub private: bool,
    /// Whether the owner is an organization (GitHub owner `type == "Organization"`), as opposed to a
    /// personal user account. Lets ownership decisions trust GitHub's own classification rather than
    /// guessing from the login.
    pub owner_is_org: bool,
}
