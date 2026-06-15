use serde::Deserialize;

/// A single entry from the `check_runs` array of `GET /commits/{sha}/check-runs` (subset).
#[derive(Debug, Deserialize)]
pub(crate) struct WireCheckRun {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
}

impl From<WireCheckRun> for domain::CheckRun {
    fn from(w: WireCheckRun) -> Self {
        domain::CheckRun {
            name: w.name,
            status: w.status,
            conclusion: w.conclusion,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_check_run_maps_to_domain() {
        let json = r#"{"name":"build","status":"completed","conclusion":"success","extra":true}"#;
        let wire: WireCheckRun = serde_json::from_str(json).unwrap();
        let run: domain::CheckRun = wire.into();
        assert_eq!(
            run,
            domain::CheckRun {
                name: "build".to_string(),
                status: "completed".to_string(),
                conclusion: Some("success".to_string()),
            }
        );
    }

    #[test]
    fn wire_check_run_running_has_no_conclusion() {
        let json = r#"{"name":"clippy","status":"in_progress","conclusion":null}"#;
        let wire: WireCheckRun = serde_json::from_str(json).unwrap();
        let run: domain::CheckRun = wire.into();
        assert_eq!(run.status, "in_progress");
        assert_eq!(run.conclusion, None);
    }
}
