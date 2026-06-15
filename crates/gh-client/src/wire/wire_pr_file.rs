use serde::Deserialize;

/// GitHub's changed-file item from `GET /repos/{repo}/pulls/{n}/files` (subset).
///
/// Each item describes one file touched by the PR. GitHub also returns `status`, `additions`,
/// `deletions`, `changes`, `sha`, `patch`, … — all ignored here (serde drops unknown fields). Only
/// the path is the changed-paths signal Phase 4 keys on.
#[derive(Debug, Deserialize)]
pub(crate) struct WirePrFile {
    pub filename: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_pr_file_extracts_filename_and_tolerates_extra_fields() {
        let json = r#"{
            "filename": "src/auth/login.rs",
            "status": "modified",
            "additions": 12,
            "deletions": 3,
            "changes": 15,
            "sha": "abc123"
        }"#;
        let wire: WirePrFile = serde_json::from_str(json).unwrap();
        assert_eq!(wire.filename, "src/auth/login.rs");
    }

    #[test]
    fn wire_pr_file_minimal_payload() {
        let json = r#"{"filename":"Cargo.lock"}"#;
        let wire: WirePrFile = serde_json::from_str(json).unwrap();
        assert_eq!(wire.filename, "Cargo.lock");
    }
}
