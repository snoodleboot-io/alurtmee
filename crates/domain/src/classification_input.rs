/// The PR-derived inputs to category classification, assembled per PR (borrowed, transient).
///
/// Bundling the signals keeps [`classify_category`](crate::classify_category) to three arguments
/// (input, label map, optional correction) instead of a long positional list.
#[derive(Debug, Clone, Copy)]
pub struct ClassificationInput<'a> {
    /// Author login (feeds the Dependabot signal).
    pub author_login: &'a str,
    /// PR title (feeds the title-prefix signal).
    pub title: &'a str,
    /// Head branch name (feeds the branch-prefix signal).
    pub head_ref: &'a str,
    /// Label names on the PR (feed the highest-precedence label signal).
    pub labels: &'a [String],
    /// Changed file paths from `/pulls/{n}/files` (feed the sensitive-paths signal).
    pub changed_paths: &'a [String],
}
