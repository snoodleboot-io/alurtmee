use serde::Deserialize;

/// A single entry from the `workflow_runs` array of `GET /repos/{repo}/actions/runs` (subset).
///
/// GitHub's run object is large; only the fields CI-timing analysis needs are modelled (extra
/// fields are ignored). `conclusion` is `null` until the run completes; `run_started_at` and
/// `updated_at` are RFC3339 UTC strings whose difference yields the run's wall-clock duration.
#[derive(Debug, Deserialize)]
pub(crate) struct WireWorkflowRun {
    pub id: u64,
    pub name: String,
    // Modelled to mirror GitHub's payload (the run's lifecycle state, e.g. `in_progress`), but the
    // domain derives completion from `conclusion` being `Some`, so this field is not read directly.
    #[allow(dead_code)]
    pub status: String,
    pub conclusion: Option<String>,
    pub run_started_at: Option<String>,
    pub updated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_workflow_run_parses_completed_and_ignores_extras() {
        let json = r#"{
            "id": 42,
            "name": "CI",
            "status": "completed",
            "conclusion": "success",
            "run_started_at": "2026-06-15T00:00:00Z",
            "updated_at": "2026-06-15T00:00:30Z",
            "head_branch": "main",
            "event": "push"
        }"#;
        let wire: WireWorkflowRun = serde_json::from_str(json).unwrap();
        assert_eq!(wire.id, 42);
        assert_eq!(wire.name, "CI");
        assert_eq!(wire.status, "completed");
        assert_eq!(wire.conclusion.as_deref(), Some("success"));
        assert_eq!(wire.run_started_at.as_deref(), Some("2026-06-15T00:00:00Z"));
        assert_eq!(wire.updated_at.as_deref(), Some("2026-06-15T00:00:30Z"));
    }

    #[test]
    fn wire_workflow_run_in_progress_has_null_conclusion() {
        let json = r#"{
            "id": 7,
            "name": "CI",
            "status": "in_progress",
            "conclusion": null,
            "run_started_at": "2026-06-15T00:00:00Z"
        }"#;
        let wire: WireWorkflowRun = serde_json::from_str(json).unwrap();
        assert_eq!(wire.status, "in_progress");
        assert_eq!(wire.conclusion, None);
        assert_eq!(wire.updated_at, None);
    }
}
