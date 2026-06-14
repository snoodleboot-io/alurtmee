use std::time::Duration;

use domain::{ChangeEvent, RateLimitState};

/// The result of a single poll cycle across all selected repositories.
///
/// `changed` drives the adaptive cadence (reset to base on change, back off otherwise);
/// `poll_interval` carries the server's `X-Poll-Interval` hint (a cadence floor); `rate_limit` is
/// the latest budget snapshot for logging/back-pressure.
#[derive(Debug, Clone, Default)]
pub struct PollOutcome {
    /// Change events detected this cycle, in fetch order.
    pub events: Vec<ChangeEvent>,
    /// Whether any repository reported a change (a non-304 with a non-empty diff).
    pub changed: bool,
    /// The largest server `X-Poll-Interval` hint seen this cycle, if any.
    pub poll_interval: Option<Duration>,
    /// The most recent rate-limit snapshot seen this cycle, if any.
    pub rate_limit: Option<RateLimitState>,
}
