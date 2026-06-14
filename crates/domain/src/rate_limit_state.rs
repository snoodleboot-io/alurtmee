/// A snapshot of GitHub's rate-limit accounting, parsed from `X-RateLimit-*` response headers.
///
/// Tracked so the poller can back off before exhaustion and so the deferred §10 pass can assert
/// the AD-1 invariant: a real `304` must leave `remaining` unchanged (free idle polling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitState {
    /// Total requests permitted in the current window (`X-RateLimit-Limit`).
    pub limit: u64,
    /// Requests still available in the current window (`X-RateLimit-Remaining`).
    pub remaining: u64,
    /// Unix epoch seconds at which the window resets (`X-RateLimit-Reset`).
    pub reset_at: u64,
}

impl RateLimitState {
    /// Whether the budget is exhausted (no requests remaining).
    pub fn is_exhausted(&self) -> bool {
        self.remaining == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_exhausted_only_at_zero_remaining() {
        let base = RateLimitState {
            limit: 5000,
            remaining: 1,
            reset_at: 1_700_000_000,
        };
        assert!(!base.is_exhausted());
        assert!(RateLimitState {
            remaining: 0,
            ..base
        }
        .is_exhausted());
    }
}
