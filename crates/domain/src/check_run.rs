use serde::{Deserialize, Serialize};

/// A single CI check on a PR's head commit (`GET /commits/{sha}/check-runs`).
///
/// `status` is the lifecycle (`queued`, `in_progress`, `completed`); `conclusion` is the outcome
/// once completed (`success`, `failure`, `neutral`, `cancelled`, `timed_out`, …) and is `None`
/// while still running. [`TestSummary`] reconciles a set of these (and the legacy combined status)
/// into an overall pass/fail/pending verdict.
///
/// [`TestSummary`]: crate::TestSummary
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRun {
    /// The check's name (e.g. `build`, `clippy`).
    pub name: String,
    /// Lifecycle status: `queued` | `in_progress` | `completed`.
    pub status: String,
    /// Outcome once completed, or `None` while still running.
    pub conclusion: Option<String>,
}
