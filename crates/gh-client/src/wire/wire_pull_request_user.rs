use serde::Deserialize;

/// The nested `user` object inside a pulls list item. The `login` populates the PR author; the
/// account `type` (renamed from the reserved word) feeds human-vs-bot classification.
#[derive(Debug, Deserialize)]
pub(crate) struct WirePullRequestUser {
    pub login: String,
    #[serde(rename = "type", default)]
    pub account_type: String,
}
