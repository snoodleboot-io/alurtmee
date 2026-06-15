use serde::Deserialize;

/// The nested `head` object of a pull request payload. Only the commit `sha` is needed — it keys
/// the check-runs/status enrichment fetch.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct WireHead {
    #[serde(default)]
    pub sha: String,
}
