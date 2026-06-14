use serde::{Deserialize, Serialize};

/// The authenticated GitHub user, as proven by a successful `GET /user`.
///
/// Phase 1 needs only enough to confirm "the token works and belongs to *this* account" and to
/// label the UI. Wider profile fields are intentionally omitted (scope discipline) — `gh-client`
/// maps GitHub's wire payload onto this clean type so `domain` never depends on GitHub JSON quirks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Stable numeric account id (never recycled by GitHub).
    pub id: u64,
    /// Account login (e.g. `octocat`).
    pub login: String,
}
