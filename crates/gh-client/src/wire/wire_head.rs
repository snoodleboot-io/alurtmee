use serde::Deserialize;

/// The nested `head` object of a pull request payload. The commit `sha` keys the check-runs/status
/// enrichment fetch; the branch `ref` feeds the branch-prefix classification signal.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct WireHead {
    #[serde(default)]
    pub sha: String,
    #[serde(rename = "ref", default)]
    pub ref_name: String,
}
