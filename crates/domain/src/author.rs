use serde::{Deserialize, Serialize};

use crate::author_kind::AuthorKind;

/// The author of a pull request, comment, or review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    /// GitHub login (e.g. `octocat`, `dependabot[bot]`).
    pub login: String,
    /// Classified account kind.
    pub kind: AuthorKind,
}
