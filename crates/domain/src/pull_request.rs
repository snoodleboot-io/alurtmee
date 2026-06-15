use serde::{Deserialize, Serialize};

use crate::pr_id::PrId;

/// An open pull request as surfaced by the cheap change-detection tier (`GET .../pulls?state=open`).
///
/// `updated_at` is GitHub's last-touched timestamp; it is the field the diff engine compares to
/// decide a PR was *updated* (GitHub bumps it on any change). Classification (human/bot,
/// feature/security) is deliberately absent — that is Phase 4 enrichment, not change-detection.
/// `gh-client` builds this from GitHub's wire payload so this stays a flat, persistable value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    /// Stable identity (`repo` + `number`).
    pub id: PrId,
    /// PR title.
    pub title: String,
    /// Author login (unclassified at this tier).
    pub author: String,
    /// Whether the PR is a draft.
    pub draft: bool,
    /// GitHub's `updated_at` (ISO-8601); the change-detection signal.
    pub updated_at: String,
    /// Web URL for opening the PR in a browser.
    pub url: String,
    /// SHA of the PR's head commit — the key for fetching check-runs/status during enrichment.
    pub head_sha: String,
}
