use serde::{Deserialize, Serialize};

/// Stable identity of a pull request: its repository slug plus the per-repo number.
///
/// `(repo, number)` is GitHub's human-meaningful key and is stable across edits, so the poller
/// uses it to match a cached PR against a freshly fetched one when diffing. It is `Hash`/`Ord` so
/// it can key a map and sort deterministically.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PrId {
    /// Owner/name slug, e.g. `octocat/hello`.
    pub repo: String,
    /// Pull request number within the repository.
    pub number: u64,
}

impl PrId {
    /// Construct a `PrId` from a repo slug and number.
    pub fn new(repo: impl Into<String>, number: u64) -> Self {
        Self {
            repo: repo.into(),
            number,
        }
    }
}
