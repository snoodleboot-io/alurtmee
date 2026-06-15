use serde::{Deserialize, Serialize};

/// Which GitHub endpoint a [`Comment`] came from.
///
/// PRs carry two distinct comment streams: top-level *issue* comments (`/issues/{n}/comments`) and
/// inline *review* comments on the diff (`/pulls/{n}/comments`). We merge them into one attributed
/// thread but keep the origin so the UI can label inline-vs-conversation comments.
///
/// [`Comment`]: crate::Comment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentKind {
    /// A top-level conversation comment (`/issues/{n}/comments`).
    Issue,
    /// An inline comment on the diff (`/pulls/{n}/comments`).
    Review,
}
