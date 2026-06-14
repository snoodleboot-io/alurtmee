use std::time::Duration;

use crate::pr_outcome::PrOutcome;

/// The full result of [`GhClient::list_open_prs`](crate::GhClient::list_open_prs).
///
/// Beyond the [`PrOutcome`] (modified vs. not), it carries the side-channel data the poller needs
/// to drive the next cycle: the `ETag` to replay as `If-None-Match`, the parsed rate-limit snapshot
/// for back-off decisions, and GitHub's suggested `X-Poll-Interval` so we never poll faster than
/// GitHub asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPrsResult {
    /// Whether the PR list changed, and the new list when it did.
    pub outcome: PrOutcome,
    /// The `ETag` to persist and replay as `If-None-Match` on the next request, if GitHub sent one.
    pub etag: Option<String>,
    /// Rate-limit accounting parsed from `X-RateLimit-*`, when all three headers were present.
    pub rate_limit: Option<domain::RateLimitState>,
    /// GitHub's suggested minimum poll interval (`X-Poll-Interval`, seconds), when present.
    pub poll_interval: Option<Duration>,
}
