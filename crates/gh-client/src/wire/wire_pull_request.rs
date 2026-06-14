use serde::Deserialize;

use super::wire_pull_request_user::WirePullRequestUser;

/// GitHub's `GET /repos/{owner}/{repo}/pulls` list item (subset). Extra fields are ignored.
///
/// The repo slug is *not* part of this payload (the URL carries it), so unlike the other wire
/// types this maps via [`into_pull_request`](WirePullRequest::into_pull_request) — which takes the
/// `repo` — rather than a `From` impl. The author lives in a nested `user` object; we flatten its
/// `login` on the way to `domain::PullRequest`.
#[derive(Debug, Deserialize)]
pub(crate) struct WirePullRequest {
    pub number: u64,
    pub title: String,
    pub user: WirePullRequestUser,
    pub draft: bool,
    pub updated_at: String,
    pub html_url: String,
}

impl WirePullRequest {
    /// Map this wire item into a `domain::PullRequest`, stamping in the `repo` slug from the URL.
    pub(crate) fn into_pull_request(self, repo: &str) -> domain::PullRequest {
        domain::PullRequest {
            id: domain::PrId::new(repo, self.number),
            title: self.title,
            author: self.user.login,
            draft: self.draft,
            updated_at: self.updated_at,
            url: self.html_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_pull_request_flattens_user_and_stamps_repo() {
        let json = r#"{
            "number": 17,
            "title": "Add widget",
            "user": {"login": "octocat", "id": 1, "type": "User"},
            "draft": false,
            "updated_at": "2026-06-14T10:00:00Z",
            "html_url": "https://github.com/octocat/hello/pull/17",
            "state": "open",
            "extra": "ignored"
        }"#;
        let wire: WirePullRequest = serde_json::from_str(json).unwrap();
        let pr = wire.into_pull_request("octocat/hello");
        assert_eq!(pr.id, domain::PrId::new("octocat/hello", 17));
        assert_eq!(pr.title, "Add widget");
        assert_eq!(pr.author, "octocat");
        assert!(!pr.draft);
        assert_eq!(pr.updated_at, "2026-06-14T10:00:00Z");
        assert_eq!(pr.url, "https://github.com/octocat/hello/pull/17");
    }
}
