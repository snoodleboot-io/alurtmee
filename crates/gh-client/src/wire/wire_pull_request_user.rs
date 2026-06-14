use serde::Deserialize;

/// The nested `user` object inside a pulls list item. Only the login is needed to populate
/// `domain::PullRequest::author`.
#[derive(Debug, Deserialize)]
pub(crate) struct WirePullRequestUser {
    pub login: String,
}
