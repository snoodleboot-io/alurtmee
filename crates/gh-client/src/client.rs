use serde::de::DeserializeOwned;

use crate::error::GhError;
use crate::wire::{WireOrg, WireRepo, WireUser};

const USER_AGENT: &str = "alurtmee";
const ACCEPT: &str = "application/vnd.github+json";
const API_VERSION: &str = "2022-11-28";

/// GitHub REST API client.
///
/// Holds the (trailing-slash-trimmed) base URL, the auth token, and a shared `reqwest::Client`.
/// The base URL is injectable so tests can point the client at a `wiremock` server (mock-first,
/// ARD R2a) — production wires `https://api.github.com`.
#[derive(Clone)]
pub struct GhClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

// SECURITY: hand-written Debug that REDACTS the token. The PAT must never reach Debug output or
// logs, so `GhClient` deliberately does not derive Debug.
impl std::fmt::Debug for GhClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GhClient")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl GhClient {
    /// Construct a client pointed at a GitHub REST base URL (e.g. `https://api.github.com`),
    /// authenticating with `token`. Any trailing `/` on the base URL is trimmed.
    ///
    /// Returns [`GhError::Network`] if the underlying HTTP client fails to build.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Result<Self, GhError> {
        let http = reqwest::Client::builder().build()?;
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Ok(Self {
            base_url,
            token: token.into(),
            http,
        })
    }

    /// The configured REST base URL (trailing slash trimmed).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Validate the token by fetching the authenticated user (`GET /user`).
    pub async fn validate(&self) -> Result<domain::User, GhError> {
        let url = format!("{}/user", self.base_url);
        let wire: WireUser = self.get_json(&url).await?;
        Ok(wire.into())
    }

    /// List organizations the authenticated user belongs to (`GET /user/orgs`, paginated).
    pub async fn list_orgs(&self) -> Result<Vec<domain::Org>, GhError> {
        let url = format!("{}/user/orgs", self.base_url);
        let wires: Vec<WireOrg> = self.get_paginated(url).await?;
        Ok(wires.into_iter().map(Into::into).collect())
    }

    /// List repositories the authenticated user can access (`GET /user/repos`, paginated).
    pub async fn list_user_repos(&self) -> Result<Vec<domain::Repo>, GhError> {
        let url = format!(
            "{}/user/repos?per_page=100&affiliation=owner,collaborator,organization_member",
            self.base_url
        );
        let wires: Vec<WireRepo> = self.get_paginated(url).await?;
        Ok(wires.into_iter().map(Into::into).collect())
    }

    /// List repositories for an organization (`GET /orgs/{org}/repos`, paginated).
    pub async fn list_org_repos(&self, org: &str) -> Result<Vec<domain::Repo>, GhError> {
        let url = format!("{}/orgs/{}/repos?per_page=100", self.base_url, org);
        let wires: Vec<WireRepo> = self.get_paginated(url).await?;
        Ok(wires.into_iter().map(Into::into).collect())
    }

    /// Issue a single GET, applying the standard headers and mapping the response status.
    async fn send(&self, url: &str) -> Result<reqwest::Response, GhError> {
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::ACCEPT, ACCEPT)
            .header("X-GitHub-Api-Version", API_VERSION)
            .send()
            .await?;
        map_status(resp)
    }

    /// GET a single JSON resource and decode it.
    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, GhError> {
        let resp = self.send(url).await?;
        decode(resp).await
    }

    /// GET a paginated JSON array, following `Link: rel="next"` until exhausted, concatenating.
    async fn get_paginated<T: DeserializeOwned>(
        &self,
        first_url: String,
    ) -> Result<Vec<T>, GhError> {
        let mut out: Vec<T> = Vec::new();
        let mut next: Option<String> = Some(first_url);
        while let Some(url) = next.take() {
            let resp = self.send(&url).await?;
            // The next URL from GitHub is absolute; request it directly. Loop terminates solely
            // when no `rel="next"` link remains.
            next = next_link(resp.headers());
            let page: Vec<T> = decode(resp).await?;
            out.extend(page);
        }
        Ok(out)
    }
}

/// Map an HTTP response's status onto a `GhError`, or pass a 2xx response through unchanged.
fn map_status(resp: reqwest::Response) -> Result<reqwest::Response, GhError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    match status.as_u16() {
        401 => Err(GhError::Unauthorized),
        403 | 429 if rate_limit_exhausted(resp.headers()) => Err(GhError::RateLimited {
            retry_after: parse_retry_after(resp.headers()),
        }),
        other => Err(GhError::Http { status: other }),
    }
}

/// Decode a (successful) response body into `T`, mapping decode failures to `GhError::Decode`.
async fn decode<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, GhError> {
    let bytes = resp.bytes().await?;
    serde_json::from_slice(&bytes).map_err(|e| GhError::Decode(e.to_string()))
}

/// True if `x-ratelimit-remaining` is present and equals `0`.
fn rate_limit_exhausted(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim() == "0")
        .unwrap_or(false)
}

/// Derive a retry-after hint (seconds) from `retry-after`, falling back to `x-ratelimit-reset`.
fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    if let Some(secs) = headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        return Some(secs);
    }
    headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
}

/// Parse a GitHub `Link` header and return the `rel="next"` URL, if any.
///
/// The header looks like:
/// `<https://api.github.com/user/repos?page=2>; rel="next", <...?page=5>; rel="last"`.
fn next_link(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let link = headers.get(reqwest::header::LINK)?.to_str().ok()?;
    for part in link.split(',') {
        let mut segments = part.split(';');
        let url_seg = segments.next()?.trim();
        let is_next = segments.any(|s| {
            let s = s.trim();
            s == r#"rel="next""# || s == "rel=next"
        });
        if is_next {
            let url = url_seg.trim_start_matches('<').trim_end_matches('>').trim();
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn user_repos_page1() -> &'static str {
        r#"[
            {"id":1,"name":"alpha","full_name":"octocat/alpha","private":false,"owner":{"login":"octocat","id":100,"type":"User"}},
            {"id":2,"name":"beta","full_name":"octocat/beta","private":true,"owner":{"login":"octocat","id":100,"type":"User"}}
        ]"#
    }

    fn user_repos_page2() -> &'static str {
        r#"[
            {"id":3,"name":"gamma","full_name":"octocat/gamma","private":false,"owner":{"login":"octocat","id":100,"type":"User"}}
        ]"#
    }

    #[tokio::test]
    async fn validate_returns_user_and_sends_auth_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .and(header("authorization", "Bearer dummy-token"))
            .and(header("user-agent", "alurtmee"))
            .and(header("accept", "application/vnd.github+json"))
            .and(header("x-github-api-version", "2022-11-28"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"id":7,"login":"octocat","type":"User","name":"The Octocat"}"#,
            ))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let user = client.validate().await.unwrap();
        assert_eq!(user.id, 7);
        assert_eq!(user.login, "octocat");
    }

    #[tokio::test]
    async fn list_user_repos_follows_pagination_across_two_pages() {
        let server = MockServer::start().await;
        let next_url = format!("{}/user/repos?page=2", server.uri());

        Mock::given(method("GET"))
            .and(path("/user/repos"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user_repos_page2()))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/user/repos"))
            .and(query_param(
                "affiliation",
                "owner,collaborator,organization_member",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Link", format!(r#"<{next_url}>; rel="next""#).as_str())
                    .set_body_string(user_repos_page1()),
            )
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let repos = client.list_user_repos().await.unwrap();
        assert_eq!(repos.len(), 3, "should concatenate both pages");
        let names: Vec<&str> = repos.iter().map(|r| r.full_name.as_str()).collect();
        assert!(names.contains(&"octocat/alpha"));
        assert!(names.contains(&"octocat/gamma"));
    }

    #[tokio::test]
    async fn unauthorized_status_maps_to_unauthorized_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(
                ResponseTemplate::new(401).set_body_string(r#"{"message":"Bad credentials"}"#),
            )
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "bad-token").unwrap();
        let err = client.validate().await.unwrap_err();
        assert!(matches!(err, GhError::Unauthorized));
    }

    #[tokio::test]
    async fn rate_limited_response_maps_to_rate_limited_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-remaining", "0")
                    .insert_header("retry-after", "42")
                    .set_body_string(r#"{"message":"API rate limit exceeded"}"#),
            )
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let err = client.validate().await.unwrap_err();
        match err {
            GhError::RateLimited { retry_after } => assert_eq!(retry_after, Some(42)),
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unexpected_status_maps_to_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let err = client.validate().await.unwrap_err();
        assert!(matches!(err, GhError::Http { status: 500 }));
    }

    #[test]
    fn debug_redacts_token() {
        let client = GhClient::new("https://api.github.com", "super-secret-pat").unwrap();
        let rendered = format!("{client:?}");
        assert!(
            !rendered.contains("super-secret-pat"),
            "token leaked: {rendered}"
        );
        assert!(rendered.contains("<redacted>"));
        assert!(rendered.contains("https://api.github.com"));
    }

    #[test]
    fn new_trims_trailing_slash() {
        let client = GhClient::new("https://api.github.com/", "t").unwrap();
        assert_eq!(client.base_url(), "https://api.github.com");
    }

    #[test]
    fn next_link_extracts_next_rel() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::LINK,
            r#"<https://api.github.com/user/repos?page=2>; rel="next", <https://api.github.com/user/repos?page=5>; rel="last""#
                .parse()
                .unwrap(),
        );
        assert_eq!(
            next_link(&headers).as_deref(),
            Some("https://api.github.com/user/repos?page=2")
        );
    }

    #[test]
    fn next_link_returns_none_without_next() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::LINK,
            r#"<https://api.github.com/user/repos?page=1>; rel="prev""#
                .parse()
                .unwrap(),
        );
        assert!(next_link(&headers).is_none());
    }
}
