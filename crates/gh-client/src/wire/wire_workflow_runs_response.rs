use serde::Deserialize;

use crate::wire::WireWorkflowRun;

/// The envelope returned by `GET /repos/{repo}/actions/runs`: an object
/// `{ total_count, workflow_runs }` (NOT a bare array), so it is decoded with `get_json` rather than
/// the paginated array helper. `total_count` is ignored — only the `workflow_runs` array is needed.
#[derive(Debug, Deserialize)]
pub(crate) struct WireWorkflowRunsResponse {
    pub workflow_runs: Vec<WireWorkflowRun>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_workflow_runs_response_extracts_array() {
        let json = r#"{
            "total_count": 2,
            "workflow_runs": [
                {"id":1,"name":"CI","status":"completed","conclusion":"success",
                 "run_started_at":"2026-06-15T00:00:00Z","updated_at":"2026-06-15T00:00:30Z"},
                {"id":2,"name":"CI","status":"in_progress","conclusion":null,
                 "run_started_at":"2026-06-15T01:00:00Z"}
            ]
        }"#;
        let wire: WireWorkflowRunsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(wire.workflow_runs.len(), 2);
    }
}
