use serde::{Deserialize, Serialize};

/// A submitted review on a pull request (`GET /pulls/{n}/reviews`).
///
/// `state` is GitHub's raw review state (`APPROVED`, `CHANGES_REQUESTED`, `COMMENTED`,
/// `DISMISSED`, …) kept as a string: the UI displays it and we don't want a closed enum to drop
/// states GitHub may add. `author` is preserved for Phase 4 classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    /// Login of the reviewer.
    pub author: String,
    /// GitHub review state, verbatim (e.g. `APPROVED`, `CHANGES_REQUESTED`).
    pub state: String,
    /// When the review was submitted (ISO-8601); empty if GitHub omitted it (e.g. pending).
    pub submitted_at: String,
}
