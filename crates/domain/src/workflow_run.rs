use serde::{Deserialize, Serialize};

/// A GitHub Actions workflow run, reduced to what CI-timing analysis needs.
///
/// `conclusion` is `Some` only once the run has completed (`success`, `failure`, `cancelled`, …);
/// `duration_secs` is meaningful only for completed runs (it is the wall-clock time the run took,
/// computed by `gh-client` from the run's start/finish timestamps). Incomplete runs carry
/// `conclusion: None` and are excluded from baselines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRun {
    /// GitHub run id.
    pub id: u64,
    /// Owner/name slug the run belongs to.
    pub repo: String,
    /// Workflow name (e.g. `CI`), the grouping key for baselines.
    pub workflow: String,
    /// Outcome once completed; `None` while still running.
    pub conclusion: Option<String>,
    /// Wall-clock duration in seconds (0 until completed).
    pub duration_secs: u64,
}

impl WorkflowRun {
    /// Whether the run has completed (and thus has a usable duration/conclusion).
    pub fn is_completed(&self) -> bool {
        self.conclusion.is_some()
    }

    /// Whether the run concluded in failure (`failure`, `timed_out`, `startup_failure`).
    pub fn is_failure(&self) -> bool {
        matches!(
            self.conclusion.as_deref(),
            Some("failure") | Some("timed_out") | Some("startup_failure")
        )
    }
}
