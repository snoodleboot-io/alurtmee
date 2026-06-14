/// Errors surfaced by the GitHub client.
///
/// Covers the full Phase 1 surface: configuration, transport, auth, rate limiting, unexpected
/// HTTP status, and JSON decode failures. Library code never panics — every fallible path returns
/// one of these variants via `?`.
#[derive(Debug, thiserror::Error)]
pub enum GhError {
    /// The client was handed invalid configuration (e.g. a malformed base URL).
    #[error("invalid GitHub client configuration: {0}")]
    Config(String),

    /// A transport-level failure: connection refused, TLS error, timeout, etc.
    #[error("network error talking to GitHub: {0}")]
    Network(#[from] reqwest::Error),

    /// GitHub rejected the credentials (HTTP 401) — the token is missing, invalid, or expired.
    #[error("GitHub rejected the token (unauthorized)")]
    Unauthorized,

    /// The primary or secondary rate limit was hit (HTTP 403/429 with no remaining quota).
    ///
    /// `retry_after` is the number of seconds to wait before retrying, parsed from `retry-after`
    /// or derived from `x-ratelimit-reset` when present; `None` if GitHub gave no hint.
    #[error("GitHub rate limit exceeded (retry after: {retry_after:?})")]
    RateLimited {
        /// Seconds to wait before retrying, if GitHub supplied a hint.
        retry_after: Option<u64>,
    },

    /// GitHub returned an unexpected non-success status not covered by a more specific variant.
    #[error("unexpected HTTP status from GitHub: {status}")]
    Http {
        /// The HTTP status code GitHub returned.
        status: u16,
    },

    /// The response body could not be decoded into the expected shape.
    #[error("failed to decode GitHub response: {0}")]
    Decode(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_display_contains_inner_message() {
        let err = GhError::Config("missing token".to_string());
        assert!(err.to_string().contains("missing token"));
    }

    #[test]
    fn unauthorized_display_is_descriptive() {
        assert!(GhError::Unauthorized.to_string().contains("unauthorized"));
    }

    #[test]
    fn rate_limited_display_includes_retry_after() {
        let err = GhError::RateLimited {
            retry_after: Some(60),
        };
        assert!(err.to_string().contains("60"));
    }

    #[test]
    fn http_display_includes_status() {
        let err = GhError::Http { status: 500 };
        assert!(err.to_string().contains("500"));
    }
}
