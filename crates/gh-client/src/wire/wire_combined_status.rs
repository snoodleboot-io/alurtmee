use serde::Deserialize;

/// The legacy combined commit status from `GET /commits/{sha}/status` (subset).
///
/// `state` is the rolled-up legacy status: `success` | `failure` | `pending`. It can only *raise*
/// severity when reconciled with the Checks API (see [`domain::TestSummary::reconcile`]).
#[derive(Debug, Deserialize)]
pub(crate) struct WireCombinedStatus {
    pub state: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_combined_status_reads_state() {
        let json = r#"{"state":"failure","statuses":[],"sha":"abc"}"#;
        let wire: WireCombinedStatus = serde_json::from_str(json).unwrap();
        assert_eq!(wire.state, "failure");
    }
}
