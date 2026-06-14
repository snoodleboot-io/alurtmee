use crate::pr_id::PrId;
use crate::pull_request::PullRequest;

/// A change detected by diffing a fresh poll against the cached state, emitted to the UI.
///
/// The retained-mode UI consumes these incrementally (only the affected rows redraw) rather than
/// re-rendering the whole list each cycle — the reason the poller emits a *diff* of events instead
/// of a full snapshot (ARD AD-7, NFR2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeEvent {
    /// A pull request became visible since the previous cycle.
    Added(PullRequest),
    /// A previously seen pull request changed (its `updated_at` advanced).
    Updated(PullRequest),
    /// A pull request is no longer open; carries its identity.
    Removed(PrId),
}
