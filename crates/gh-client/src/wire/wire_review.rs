use serde::Deserialize;

use crate::wire::WirePullRequestUser;

/// GitHub's review payload from `GET /repos/{repo}/pulls/{n}/reviews` (subset).
///
/// `submitted_at` is optional: GitHub omits it for pending reviews. The reviewer login is nested
/// under `user`, reused from [`WirePullRequestUser`].
#[derive(Debug, Deserialize)]
pub(crate) struct WireReview {
    pub user: WirePullRequestUser,
    pub state: String,
    pub submitted_at: Option<String>,
}

impl From<WireReview> for domain::Review {
    fn from(w: WireReview) -> Self {
        domain::Review {
            author: w.user.login,
            state: w.state,
            submitted_at: w.submitted_at.unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_review_maps_to_domain() {
        let json = r#"{"user":{"login":"alice"},"state":"APPROVED","submitted_at":"2026-06-14T09:00:00Z","extra":"ignored"}"#;
        let wire: WireReview = serde_json::from_str(json).unwrap();
        let review: domain::Review = wire.into();
        assert_eq!(
            review,
            domain::Review {
                author: "alice".to_string(),
                state: "APPROVED".to_string(),
                submitted_at: "2026-06-14T09:00:00Z".to_string(),
            }
        );
    }

    #[test]
    fn wire_review_missing_submitted_at_defaults_empty() {
        let json = r#"{"user":{"login":"bob"},"state":"PENDING"}"#;
        let wire: WireReview = serde_json::from_str(json).unwrap();
        let review: domain::Review = wire.into();
        assert_eq!(review.author, "bob");
        assert_eq!(review.submitted_at, "");
    }
}
