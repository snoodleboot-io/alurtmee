use serde::{Deserialize, Serialize};

/// A GitHub organization the authenticated user belongs to (`GET /user/orgs`).
///
/// Used to offer org-scoped repository discovery in the settings picker.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Org {
    /// Stable numeric organization id.
    pub id: u64,
    /// Organization login (the `{owner}` segment in `owner/repo`).
    pub login: String,
}
