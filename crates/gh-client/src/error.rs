/// Errors surfaced by the GitHub client.
///
/// Phase 0 defines the seam with the one variant that already has meaning (configuration);
/// transport, auth, and rate-limit variants attach in Phase 1 alongside `reqwest`.
#[derive(Debug, thiserror::Error)]
pub enum GhError {
    #[error("invalid GitHub client configuration: {0}")]
    Config(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_display_contains_inner_message() {
        let err = GhError::Config("missing token".to_string());
        assert!(err.to_string().contains("missing token"));
    }
}
