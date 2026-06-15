use serde::Deserialize;

use crate::wire::WireCheckRun;

/// The envelope returned by `GET /commits/{sha}/check-runs`: an object `{ total_count, check_runs }`
/// (NOT a bare array), so it is decoded with `get_json` rather than the paginated array helper.
/// `total_count` is ignored — only the `check_runs` array is needed.
#[derive(Debug, Deserialize)]
pub(crate) struct WireCheckRunsResponse {
    pub check_runs: Vec<WireCheckRun>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_check_runs_response_extracts_array() {
        let json = r#"{
            "total_count": 2,
            "check_runs": [
                {"name":"build","status":"completed","conclusion":"success"},
                {"name":"test","status":"completed","conclusion":"failure"}
            ]
        }"#;
        let wire: WireCheckRunsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(wire.check_runs.len(), 2);
    }
}
