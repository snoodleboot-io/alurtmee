use serde::{Deserialize, Serialize};

use crate::comment_kind::CommentKind;

/// A comment on a pull request, attributed to its author and tagged with its origin.
///
/// The `author` is retained (not discarded) because Phase 4 classification keys on commenter
/// identity (human vs bot, etc.); enrichment must preserve it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    /// Login of the commenter.
    pub author: String,
    /// Whether this is an issue (conversation) or review (inline) comment.
    pub kind: CommentKind,
    /// The comment body text.
    pub body: String,
    /// GitHub `created_at` timestamp (ISO-8601).
    pub created_at: String,
}
