use std::time::Duration;

/// Adaptive polling cadence policy: how long to wait before the next poll cycle.
///
/// **Why adaptive, not a fixed interval (§3.6):** Alurtmee should be cheap when idle and responsive
/// when active. After consecutive no-change cycles the interval backs off exponentially toward
/// `max` (low idle cost, fewer wasted conditional requests); a detected change resets it to `base`
/// (responsive while the user is active). The server's `X-Poll-Interval` hint is always respected
/// as a floor so we never poll faster than GitHub asks. Jitter (thundering-herd avoidance) is
/// applied by the caller, which owns the randomness source — keeping this policy pure and
/// table-testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollCadence {
    base: Duration,
    max: Duration,
}

/// Backoff is capped at `base * 2^MAX_BACKOFF_SHIFT` before the `max` clamp, bounding the shift so
/// it can never overflow regardless of how long nothing changes.
const MAX_BACKOFF_SHIFT: u32 = 6;

impl PollCadence {
    /// Construct a cadence from a `base` (active) and `max` (fully backed-off) interval. If `max`
    /// is below `base`, it is treated as equal to `base` (a degenerate but safe configuration).
    pub fn new(base: Duration, max: Duration) -> Self {
        let max = if max < base { base } else { max };
        Self { base, max }
    }

    /// The active (no-backoff) interval.
    pub fn base(&self) -> Duration {
        self.base
    }

    /// The maximum (fully backed-off) interval.
    pub fn max(&self) -> Duration {
        self.max
    }

    /// Compute the next interval given how many consecutive cycles saw no change and an optional
    /// server `X-Poll-Interval` hint. Exponential backoff (`base * 2^n`) clamped to `max`, then
    /// raised to at least the server hint.
    pub fn interval(&self, consecutive_unchanged: u32, server_hint: Option<Duration>) -> Duration {
        let shift = consecutive_unchanged.min(MAX_BACKOFF_SHIFT);
        let factor = 1u32 << shift;
        let backed = self.base.saturating_mul(factor).min(self.max);
        match server_hint {
            Some(hint) => backed.max(hint),
            None => backed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn zero_unchanged_yields_base() {
        let cadence = PollCadence::new(secs(30), secs(300));
        assert_eq!(cadence.interval(0, None), secs(30));
    }

    #[test]
    fn backoff_is_exponential_then_clamped_to_max() {
        let cadence = PollCadence::new(secs(30), secs(300));
        assert_eq!(cadence.interval(1, None), secs(60), "30*2");
        assert_eq!(cadence.interval(2, None), secs(120), "30*4");
        assert_eq!(cadence.interval(3, None), secs(240), "30*8");
        assert_eq!(cadence.interval(4, None), secs(300), "30*16 clamped to max");
        assert_eq!(
            cadence.interval(100, None),
            secs(300),
            "huge count stays clamped, no overflow"
        );
    }

    #[test]
    fn server_hint_acts_as_a_floor() {
        let cadence = PollCadence::new(secs(30), secs(300));
        assert_eq!(
            cadence.interval(0, Some(secs(90))),
            secs(90),
            "hint raises base"
        );
        assert_eq!(
            cadence.interval(3, Some(secs(90))),
            secs(240),
            "backoff already above hint"
        );
    }

    #[test]
    fn max_below_base_is_clamped_up_to_base() {
        let cadence = PollCadence::new(secs(60), secs(10));
        assert_eq!(cadence.max(), secs(60));
        assert_eq!(cadence.interval(5, None), secs(60));
    }
}
