use serde::{Deserialize, Serialize};

use crate::comment::Comment;
use crate::pr_id::PrId;
use crate::review::Review;
use crate::test_summary::TestSummary;

/// The enrichment payload for one pull request: everything the detail view needs beyond the
/// change-detection summary.
///
/// Produced by the enrichment tier (only for PRs whose change-detection fired) and delivered to the
/// UI via [`ChangeEvent::Enriched`](crate::ChangeEvent::Enriched); persisted so the detail survives
/// a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrEnrichment {
    /// Which PR this enrichment belongs to.
    pub id: PrId,
    /// Submitted reviews.
    pub reviews: Vec<Review>,
    /// Merged, attributed comments (issue + review).
    pub comments: Vec<Comment>,
    /// Reconciled CI verdict.
    pub tests: TestSummary,
}

impl PrEnrichment {
    /// Construct an enrichment payload for `id`.
    pub fn new(id: PrId, reviews: Vec<Review>, comments: Vec<Comment>, tests: TestSummary) -> Self {
        Self {
            id,
            reviews,
            comments,
            tests,
        }
    }
}
