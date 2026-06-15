use serde::Deserialize;

use crate::wire::WirePullRequestUser;

/// GitHub's comment payload, shared by both `GET /issues/{n}/comments` and
/// `GET /pulls/{n}/comments` (subset). The two endpoints have the same relevant shape; the
/// originating [`domain::CommentKind`] is supplied by the caller via [`WireComment::into_comment`],
/// since it is not present in the JSON. The commenter login is nested under `user`, reused from
/// [`WirePullRequestUser`]; it is preserved because Phase 4 classification keys on it.
#[derive(Debug, Deserialize)]
pub(crate) struct WireComment {
    pub user: WirePullRequestUser,
    pub body: String,
    pub created_at: String,
}

impl WireComment {
    /// Map to a [`domain::Comment`], attributing the supplied `kind` (which endpoint it came from).
    pub fn into_comment(self, kind: domain::CommentKind) -> domain::Comment {
        domain::Comment {
            author: self.user.login,
            kind,
            body: self.body,
            created_at: self.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_comment_into_comment_attributes_kind_and_preserves_author() {
        let json = r#"{"user":{"login":"carol"},"body":"looks good","created_at":"2026-06-14T10:00:00Z","extra":1}"#;
        let wire: WireComment = serde_json::from_str(json).unwrap();
        let comment = wire.into_comment(domain::CommentKind::Issue);
        assert_eq!(
            comment,
            domain::Comment {
                author: "carol".to_string(),
                kind: domain::CommentKind::Issue,
                body: "looks good".to_string(),
                created_at: "2026-06-14T10:00:00Z".to_string(),
            }
        );
    }

    #[test]
    fn wire_comment_into_comment_review_kind() {
        let json = r#"{"user":{"login":"dave"},"body":"nit","created_at":"2026-06-14T11:00:00Z"}"#;
        let wire: WireComment = serde_json::from_str(json).unwrap();
        let comment = wire.into_comment(domain::CommentKind::Review);
        assert_eq!(comment.kind, domain::CommentKind::Review);
        assert_eq!(comment.author, "dave");
    }
}
