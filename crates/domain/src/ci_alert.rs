use serde::{Deserialize, Serialize};

use crate::ci_alert_kind::CiAlertKind;

/// A CI condition worth surfacing to the user: a slow run or a failed run.
///
/// Emitted by the poller for **newly-seen** runs only (de-duped at the source), shown as a badge in
/// the UI, and dispatched as a desktop notification. The `reason` is human-readable and carries no
/// token or other secret (notification hygiene).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiAlert {
    /// Owner/name slug.
    pub repo: String,
    /// Workflow name.
    pub workflow: String,
    /// The run that triggered the alert.
    pub run_id: u64,
    /// Whether it was slow or failed.
    pub kind: CiAlertKind,
    /// Human-readable explanation (no secrets).
    pub reason: String,
}

impl CiAlert {
    /// A stable de-dupe key: one alert per `(run_id, kind)`.
    pub fn dedupe_key(&self) -> (u64, CiAlertKind) {
        (self.run_id, self.kind)
    }
}
