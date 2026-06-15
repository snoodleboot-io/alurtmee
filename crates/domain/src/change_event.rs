use crate::ci_alert::CiAlert;
use crate::classification::Classification;
use crate::pr_enrichment::PrEnrichment;
use crate::pr_id::PrId;
use crate::pull_request::PullRequest;

/// A change detected by diffing a fresh poll against the cached state, emitted to the UI.
///
/// The retained-mode UI consumes these incrementally (only the affected rows redraw) rather than
/// re-rendering the whole list each cycle — the reason the poller emits a *diff* of events instead
/// of a full snapshot (ARD AD-7, NFR2).
///
/// Not `Eq`: `Classification`/`Enriched` carry a floating-point confidence, so only `PartialEq` is
/// available — which is all the consumers (event matching, test assertions) need.
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeEvent {
    /// A pull request became visible since the previous cycle.
    Added(PullRequest),
    /// A previously seen pull request changed (its `updated_at` advanced).
    Updated(PullRequest),
    /// A pull request is no longer open; carries its identity.
    Removed(PrId),
    /// Enrichment (reviews, comments, test results) for a PR that changed this cycle.
    Enriched(PrEnrichment),
    /// The human/bot + feature/security classification verdict for a PR that changed.
    Classified(Classification),
    /// A CI condition worth surfacing (slow run / failed run) for a newly-seen workflow run.
    CiAlert(CiAlert),
}
