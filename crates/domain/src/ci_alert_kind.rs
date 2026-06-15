use serde::{Deserialize, Serialize};

/// The kind of CI condition that triggered an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiAlertKind {
    /// A workflow run took longer than its baseline / threshold.
    SlowCi,
    /// A workflow run concluded in failure.
    Failure,
}
