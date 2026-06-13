/// GitHub API client.
///
/// Phase 0 carries only the configured base URL so the workspace wires together and Phase 1 has a
/// real type to hang the HTTP transport on. No network calls exist yet.
#[derive(Debug, Clone)]
pub struct GhClient {
    base_url: String,
}

impl GhClient {
    /// Construct a client pointed at a GitHub REST base URL (e.g. `https://api.github.com`).
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    /// The configured REST base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_retains_configured_base_url() {
        let client = GhClient::new("https://api.github.com");
        assert_eq!(client.base_url(), "https://api.github.com");
    }
}
