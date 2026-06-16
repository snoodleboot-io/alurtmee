use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::error::GhError;
use crate::open_prs_result::OpenPrsResult;
use crate::pr_outcome::PrOutcome;
use crate::rfc3339::parse_rfc3339_to_epoch;
use crate::wire::{
    WireCheckRunsResponse, WireCombinedStatus, WireComment, WireOrg, WirePrFile, WirePullRequest,
    WireRepo, WireReview, WireUser, WireWorkflowRun, WireWorkflowRunsResponse,
};

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

    /// List a repository's open pull requests using a conditional request (AD-1 / AD-4).
    ///
    /// `repo` is an `owner/name` slug. When `etag` is `Some`, the first request carries
    /// `If-None-Match: {etag}`; GitHub answers `304 Not Modified` (free — no rate-limit cost, no
    /// body) when nothing changed, in which case the outcome is [`PrOutcome::NotModified`].
    /// Otherwise GitHub returns `200` with the list, which is paginated (subsequent pages are
    /// fetched with plain, non-conditional GETs and concatenated) and yields
    /// [`PrOutcome::Modified`]. The returned `ETag`, rate-limit snapshot, and poll interval are
    /// always taken from the *first* response so the poller can drive its next cycle.
    pub async fn list_open_prs(
        &self,
        repo: &str,
        etag: Option<&str>,
    ) -> Result<OpenPrsResult, GhError> {
        let first_url = format!(
            "{}/repos/{}/pulls?state=open&per_page=100",
            self.base_url, repo
        );
        let resp = self.send_conditional(&first_url, etag).await?;

        // The side-channel headers come from the first response regardless of status.
        let rate_limit = parse_rate_limit(resp.headers());
        let poll_interval = parse_poll_interval(resp.headers());
        let response_etag = etag_of(resp.headers());

        // 304: cached data is current. No body to read; echo GitHub's ETag if present, else the
        // caller-supplied one (it remains valid).
        if resp.status().as_u16() == 304 {
            return Ok(OpenPrsResult {
                outcome: PrOutcome::NotModified,
                etag: response_etag.or_else(|| etag.map(str::to_string)),
                rate_limit,
                poll_interval,
            });
        }

        // 200: capture the first page's ETag, parse it, then follow `rel="next"` with plain GETs.
        let mut next = next_link(resp.headers());
        let mut prs: Vec<domain::PullRequest> = decode::<Vec<WirePullRequest>>(resp)
            .await?
            .into_iter()
            .map(|w| w.into_pull_request(repo))
            .collect();
        while let Some(url) = next.take() {
            let page_resp = self.send(&url).await?;
            next = next_link(page_resp.headers());
            let page: Vec<WirePullRequest> = decode(page_resp).await?;
            prs.extend(page.into_iter().map(|w| w.into_pull_request(repo)));
        }

        Ok(OpenPrsResult {
            outcome: PrOutcome::Modified(prs),
            etag: response_etag,
            rate_limit,
            poll_interval,
        })
    }

    /// List the submitted reviews on a pull request
    /// (`GET /repos/{repo}/pulls/{number}/reviews?per_page=100`, paginated).
    ///
    /// Enrichment is fetched only for *changed* PRs, so this is a plain (non-conditional) GET —
    /// it does not involve the ETag/304 path. `repo` is an `owner/name` slug.
    pub async fn list_reviews(
        &self,
        repo: &str,
        number: u64,
    ) -> Result<Vec<domain::Review>, GhError> {
        let url = format!(
            "{}/repos/{}/pulls/{}/reviews?per_page=100",
            self.base_url, repo, number
        );
        let wires: Vec<WireReview> = self.get_paginated(url).await?;
        Ok(wires.into_iter().map(Into::into).collect())
    }

    /// List a pull request's comments, merging both GitHub streams into one attributed thread.
    ///
    /// GitHub splits PR comments across two endpoints: top-level *issue* comments
    /// (`GET /repos/{repo}/issues/{number}/comments`) and inline *review* comments on the diff
    /// (`GET /repos/{repo}/pulls/{number}/comments`). Both are fetched (paginated) and merged with
    /// the originating [`domain::CommentKind`] attributed: issue comments first (stable order),
    /// then review comments. Each comment's author is preserved (Phase 4 classification keys on it).
    pub async fn list_comments(
        &self,
        repo: &str,
        number: u64,
    ) -> Result<Vec<domain::Comment>, GhError> {
        let issue_url = format!(
            "{}/repos/{}/issues/{}/comments?per_page=100",
            self.base_url, repo, number
        );
        let review_url = format!(
            "{}/repos/{}/pulls/{}/comments?per_page=100",
            self.base_url, repo, number
        );

        let issue_wires: Vec<WireComment> = self.get_paginated(issue_url).await?;
        let review_wires: Vec<WireComment> = self.get_paginated(review_url).await?;

        let mut comments: Vec<domain::Comment> = issue_wires
            .into_iter()
            .map(|w| w.into_comment(domain::CommentKind::Issue))
            .collect();
        comments.extend(
            review_wires
                .into_iter()
                .map(|w| w.into_comment(domain::CommentKind::Review)),
        );
        Ok(comments)
    }

    /// List the file paths a pull request changes
    /// (`GET /repos/{repo}/pulls/{number}/files?per_page=100`, paginated).
    ///
    /// Like the other enrichment fetches, the changed-paths signal is pulled only for *changed*
    /// PRs, so this is a plain (non-conditional) GET — it does not involve the ETag/304 path.
    /// `repo` is an `owner/name` slug. Each item is mapped to its `filename`; the returned `Vec`
    /// preserves GitHub's order.
    pub async fn list_changed_paths(
        &self,
        repo: &str,
        number: u64,
    ) -> Result<Vec<String>, GhError> {
        let url = format!(
            "{}/repos/{}/pulls/{}/files?per_page=100",
            self.base_url, repo, number
        );
        let wires: Vec<WirePrFile> = self.get_paginated(url).await?;
        Ok(wires.into_iter().map(|w| w.filename).collect())
    }

    /// Reconcile a PR's CI verdict for its head commit into a [`domain::TestSummary`].
    ///
    /// Combines two sources for `head_sha`: the Checks API
    /// (`GET /repos/{repo}/commits/{head_sha}/check-runs`, an object `{ total_count, check_runs }`,
    /// not a bare array) and the legacy combined commit status
    /// (`GET /repos/{repo}/commits/{head_sha}/status`, `{ state }`). Counts come from the
    /// check-runs; the legacy status can only *raise* severity. The two GETs are awaited
    /// sequentially to avoid pulling `tokio` into the production dependency set.
    pub async fn test_summary(
        &self,
        repo: &str,
        head_sha: &str,
    ) -> Result<domain::TestSummary, GhError> {
        // TODO(phase-later): paginate check-runs for PRs with >100 checks.
        let check_runs_url = format!(
            "{}/repos/{}/commits/{}/check-runs",
            self.base_url, repo, head_sha
        );
        let status_url = format!(
            "{}/repos/{}/commits/{}/status",
            self.base_url, repo, head_sha
        );

        let runs_resp: WireCheckRunsResponse = self.get_json(&check_runs_url).await?;
        let combined: WireCombinedStatus = self.get_json(&status_url).await?;

        let check_runs: Vec<domain::CheckRun> =
            runs_resp.check_runs.into_iter().map(Into::into).collect();

        Ok(domain::TestSummary::reconcile(
            &check_runs,
            Some(&combined.state),
        ))
    }

    /// List a repository's recent GitHub Actions workflow runs as [`domain::WorkflowRun`]s
    /// (`GET /repos/{repo}/actions/runs?per_page=50`).
    ///
    /// GitHub returns an object envelope `{ total_count, workflow_runs }` (not a bare array), so this
    /// decodes via [`get_json`](Self::get_json) rather than the paginated helper. Each run's
    /// `duration_secs` is derived from its `run_started_at`/`updated_at` RFC3339 timestamps:
    /// `max(0, updated - started)` when both parse, else `0`. `conclusion` is passed through (GitHub
    /// sets it only once a run completes), so in-progress runs surface as `conclusion: None` with a
    /// zero duration. `repo` is an `owner/name` slug.
    pub async fn list_workflow_runs(
        &self,
        repo: &str,
    ) -> Result<Vec<domain::WorkflowRun>, GhError> {
        // TODO(phase-later): paginate /actions/runs (a single 50-run page is enough for Phase 5).
        let url = format!("{}/repos/{}/actions/runs?per_page=50", self.base_url, repo);
        let resp: WireWorkflowRunsResponse = self.get_json(&url).await?;
        Ok(resp
            .workflow_runs
            .into_iter()
            .map(|w| map_workflow_run(w, repo))
            .collect())
    }

    /// Issue a conditional GET, applying the standard headers plus `If-None-Match` when `etag` is
    /// `Some`. Unlike [`send`](Self::send) this tolerates `304 Not Modified`: a 304 is neither a
    /// 2xx success nor an error here, so it is passed through to the caller. Genuine error statuses
    /// (401, rate-limit, other non-2xx-non-304) still map to `GhError`.
    async fn send_conditional(
        &self,
        url: &str,
        etag: Option<&str>,
    ) -> Result<reqwest::Response, GhError> {
        let mut req = self.base_get(url);
        if let Some(tag) = etag {
            req = req.header(reqwest::header::IF_NONE_MATCH, tag);
        }
        let resp = req.send().await?;
        map_status_conditional(resp)
    }

    /// Issue a single GET, applying the standard headers and mapping the response status.
    async fn send(&self, url: &str) -> Result<reqwest::Response, GhError> {
        let resp = self.base_get(url).send().await?;
        map_status(resp)
    }

    /// A GET request builder carrying the standard auth + GitHub API headers that every request
    /// shares. The one place those headers live, so a new endpoint inherits them for free.
    fn base_get(&self, url: &str) -> reqwest::RequestBuilder {
        self.http
            .get(url)
            .bearer_auth(&self.token)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::ACCEPT, ACCEPT)
            .header("X-GitHub-Api-Version", API_VERSION)
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

/// Map a GitHub workflow-run DTO onto a [`domain::WorkflowRun`], deriving its wall-clock duration.
///
/// `repo` is the slug the runs were fetched for (GitHub's per-run repository object is not modelled).
/// `duration_secs` is `max(0, updated_at - run_started_at)` when both timestamps parse, otherwise `0`
/// — so in-progress runs (missing `updated_at`, or with a `conclusion` of `null`) carry a zero
/// duration. The negative-difference clamp guards against clock skew in GitHub's timestamps.
fn map_workflow_run(w: WireWorkflowRun, repo: &str) -> domain::WorkflowRun {
    let duration_secs = match (
        w.run_started_at.as_deref().and_then(parse_rfc3339_to_epoch),
        w.updated_at.as_deref().and_then(parse_rfc3339_to_epoch),
    ) {
        (Some(started), Some(updated)) => (updated - started).max(0) as u64,
        _ => 0,
    };
    domain::WorkflowRun {
        id: w.id,
        repo: repo.to_string(),
        workflow: w.name,
        conclusion: w.conclusion,
        duration_secs,
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

/// Like [`map_status`] but for the conditional-request path: a `304 Not Modified` is passed
/// through unchanged (it is not a 2xx success, but it is *not* an error either). All other
/// non-success statuses map to a `GhError` via the same rules. Kept separate so the existing
/// non-conditional callers — which never expect 304 — continue to treat it as an unexpected status.
fn map_status_conditional(resp: reqwest::Response) -> Result<reqwest::Response, GhError> {
    if resp.status().as_u16() == 304 {
        return Ok(resp);
    }
    map_status(resp)
}

/// Parse GitHub's `X-RateLimit-*` headers into a `RateLimitState`. Returns `None` unless
/// `x-ratelimit-limit`, `x-ratelimit-remaining`, and `x-ratelimit-reset` are all present and
/// parse as `u64`.
fn parse_rate_limit(headers: &reqwest::header::HeaderMap) -> Option<domain::RateLimitState> {
    let limit = parse_u64_header(headers, "x-ratelimit-limit")?;
    let remaining = parse_u64_header(headers, "x-ratelimit-remaining")?;
    let reset_at = parse_u64_header(headers, "x-ratelimit-reset")?;
    Some(domain::RateLimitState {
        limit,
        remaining,
        reset_at,
    })
}

/// Parse GitHub's `X-Poll-Interval` (seconds) into a `Duration`, if present and numeric.
fn parse_poll_interval(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    parse_u64_header(headers, "x-poll-interval").map(Duration::from_secs)
}

/// The `ETag` response header value, if present.
fn etag_of(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
}

/// Fetch a header by name and parse its trimmed value as `u64`.
fn parse_u64_header(headers: &reqwest::header::HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
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

    fn pulls_two() -> &'static str {
        r#"[
            {"number":1,"title":"First PR","user":{"login":"alice","id":10,"type":"User"},"draft":false,"updated_at":"2026-06-14T09:00:00Z","html_url":"https://github.com/octocat/hello/pull/1","state":"open"},
            {"number":2,"title":"Second PR","user":{"login":"bob","id":11,"type":"User"},"draft":true,"updated_at":"2026-06-14T10:00:00Z","html_url":"https://github.com/octocat/hello/pull/2","state":"open"}
        ]"#
    }

    #[tokio::test]
    async fn list_open_prs_200_parses_two_prs_and_captures_etag() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls"))
            .and(query_param("state", "open"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"abc\"")
                    .set_body_string(pulls_two()),
            )
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let result = client.list_open_prs("octocat/hello", None).await.unwrap();

        assert_eq!(result.etag.as_deref(), Some("\"abc\""));
        match result.outcome {
            PrOutcome::Modified(prs) => {
                assert_eq!(prs.len(), 2);
                assert_eq!(prs[0].id, domain::PrId::new("octocat/hello", 1));
                assert_eq!(prs[0].title, "First PR");
                assert_eq!(prs[0].author, "alice");
                assert!(!prs[0].draft);
                assert_eq!(prs[1].id.number, 2);
                assert!(prs[1].draft);
            }
            other => panic!("expected Modified, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_open_prs_sends_if_none_match_and_304_yields_not_modified() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls"))
            .and(header("if-none-match", "\"abc\""))
            .respond_with(ResponseTemplate::new(304).insert_header("ETag", "\"abc\""))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let result = client
            .list_open_prs("octocat/hello", Some("\"abc\""))
            .await
            .unwrap();

        assert_eq!(result.outcome, PrOutcome::NotModified);
        assert_eq!(result.etag.as_deref(), Some("\"abc\""));
    }

    #[tokio::test]
    async fn list_open_prs_304_without_echoed_etag_falls_back_to_supplied() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls"))
            .and(header("if-none-match", "\"xyz\""))
            .respond_with(ResponseTemplate::new(304))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let result = client
            .list_open_prs("octocat/hello", Some("\"xyz\""))
            .await
            .unwrap();

        assert_eq!(result.outcome, PrOutcome::NotModified);
        assert_eq!(result.etag.as_deref(), Some("\"xyz\""));
    }

    #[tokio::test]
    async fn list_open_prs_changed_body_reflects_new_updated_at() {
        let server = MockServer::start().await;
        let changed = r#"[
            {"number":1,"title":"First PR","user":{"login":"alice","id":10,"type":"User"},"draft":false,"updated_at":"2026-06-14T12:30:00Z","html_url":"https://github.com/octocat/hello/pull/1","state":"open"}
        ]"#;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"new\"")
                    .set_body_string(changed),
            )
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let result = client.list_open_prs("octocat/hello", None).await.unwrap();
        assert_eq!(result.etag.as_deref(), Some("\"new\""));
        match result.outcome {
            PrOutcome::Modified(prs) => {
                assert_eq!(prs.len(), 1);
                assert_eq!(prs[0].updated_at, "2026-06-14T12:30:00Z");
            }
            other => panic!("expected Modified, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_open_prs_parses_rate_limit_and_poll_interval() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"abc\"")
                    .insert_header("x-ratelimit-limit", "5000")
                    .insert_header("x-ratelimit-remaining", "4999")
                    .insert_header("x-ratelimit-reset", "1700000000")
                    .insert_header("x-poll-interval", "60")
                    .set_body_string("[]"),
            )
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let result = client.list_open_prs("octocat/hello", None).await.unwrap();

        assert_eq!(
            result.rate_limit,
            Some(domain::RateLimitState {
                limit: 5000,
                remaining: 4999,
                reset_at: 1_700_000_000,
            })
        );
        assert_eq!(result.poll_interval, Some(Duration::from_secs(60)));
    }

    #[tokio::test]
    async fn list_open_prs_follows_pagination() {
        let server = MockServer::start().await;
        let next_url = format!("{}/repos/octocat/hello/pulls?page=2", server.uri());

        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"[{"number":3,"title":"Third","user":{"login":"carol","id":12,"type":"User"},"draft":false,"updated_at":"2026-06-14T11:00:00Z","html_url":"https://github.com/octocat/hello/pull/3","state":"open"}]"#,
            ))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls"))
            .and(query_param("state", "open"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"page1\"")
                    .insert_header("Link", format!(r#"<{next_url}>; rel="next""#).as_str())
                    .set_body_string(pulls_two()),
            )
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let result = client.list_open_prs("octocat/hello", None).await.unwrap();
        assert_eq!(result.etag.as_deref(), Some("\"page1\""));
        match result.outcome {
            PrOutcome::Modified(prs) => {
                assert_eq!(prs.len(), 3, "should concatenate both pages");
                let numbers: Vec<u64> = prs.iter().map(|p| p.id.number).collect();
                assert_eq!(numbers, vec![1, 2, 3]);
            }
            other => panic!("expected Modified, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_open_prs_maps_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "bad-token").unwrap();
        let err = client
            .list_open_prs("octocat/hello", None)
            .await
            .unwrap_err();
        assert!(matches!(err, GhError::Unauthorized));
    }

    #[test]
    fn parse_rate_limit_requires_all_three_headers() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ratelimit-limit", "5000".parse().unwrap());
        headers.insert("x-ratelimit-remaining", "10".parse().unwrap());
        // reset missing → None.
        assert!(parse_rate_limit(&headers).is_none());

        headers.insert("x-ratelimit-reset", "1700000000".parse().unwrap());
        assert_eq!(
            parse_rate_limit(&headers),
            Some(domain::RateLimitState {
                limit: 5000,
                remaining: 10,
                reset_at: 1_700_000_000,
            })
        );
    }

    #[test]
    fn parse_poll_interval_reads_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert!(parse_poll_interval(&headers).is_none());
        headers.insert("x-poll-interval", "60".parse().unwrap());
        assert_eq!(parse_poll_interval(&headers), Some(Duration::from_secs(60)));
    }

    #[test]
    fn etag_of_returns_header_value() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert!(etag_of(&headers).is_none());
        headers.insert(reqwest::header::ETAG, "\"abc\"".parse().unwrap());
        assert_eq!(etag_of(&headers).as_deref(), Some("\"abc\""));
    }

    #[tokio::test]
    async fn map_status_conditional_passes_304_through_while_plain_rejects_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/probe"))
            .respond_with(ResponseTemplate::new(304))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let url = format!("{}/probe", client.base_url());

        // Conditional path tolerates 304.
        let cond = client.send_conditional(&url, None).await.unwrap();
        assert_eq!(cond.status().as_u16(), 304);

        // The non-conditional path treats 304 as an unexpected status.
        let plain = client.send(&url).await;
        assert!(matches!(plain, Err(GhError::Http { status: 304 })));
    }

    // ---- enrichment: list_reviews ----

    #[tokio::test]
    async fn list_reviews_parses_two_with_author_and_state() {
        let server = MockServer::start().await;
        let body = r#"[
            {"user":{"login":"alice"},"state":"APPROVED","submitted_at":"2026-06-14T09:00:00Z"},
            {"user":{"login":"bob"},"state":"CHANGES_REQUESTED","submitted_at":"2026-06-14T10:00:00Z"}
        ]"#;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls/7/reviews"))
            .and(query_param("per_page", "100"))
            .and(header("authorization", "Bearer dummy-token"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let reviews = client.list_reviews("octocat/hello", 7).await.unwrap();
        assert_eq!(reviews.len(), 2);
        assert_eq!(reviews[0].author, "alice");
        assert_eq!(reviews[0].state, "APPROVED");
        assert_eq!(reviews[1].author, "bob");
        assert_eq!(reviews[1].state, "CHANGES_REQUESTED");
    }

    #[tokio::test]
    async fn list_reviews_empty_array_is_empty_vec() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls/7/reviews"))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let reviews = client.list_reviews("octocat/hello", 7).await.unwrap();
        assert!(reviews.is_empty());
    }

    // ---- enrichment: list_comments ----

    #[tokio::test]
    async fn list_comments_merges_issue_then_review_with_kind_attribution() {
        let server = MockServer::start().await;
        let issue_body = r#"[
            {"user":{"login":"alice"},"body":"first issue comment","created_at":"2026-06-14T09:00:00Z"},
            {"user":{"login":"bob"},"body":"second issue comment","created_at":"2026-06-14T09:30:00Z"}
        ]"#;
        let review_body = r#"[
            {"user":{"login":"carol"},"body":"inline nit","created_at":"2026-06-14T10:00:00Z"}
        ]"#;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/issues/7/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_string(issue_body))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls/7/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_string(review_body))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let comments = client.list_comments("octocat/hello", 7).await.unwrap();
        assert_eq!(comments.len(), 3);
        // Issue comments first, in order.
        assert_eq!(comments[0].kind, domain::CommentKind::Issue);
        assert_eq!(comments[0].author, "alice");
        assert_eq!(comments[1].kind, domain::CommentKind::Issue);
        assert_eq!(comments[1].author, "bob");
        // Review comment last.
        assert_eq!(comments[2].kind, domain::CommentKind::Review);
        assert_eq!(comments[2].author, "carol");
        assert_eq!(comments[2].body, "inline nit");
    }

    // ---- enrichment: list_changed_paths ----

    #[tokio::test]
    async fn list_changed_paths_returns_both_filenames_and_sends_auth() {
        let server = MockServer::start().await;
        let body = r#"[
            {"filename":"src/auth/login.rs","status":"modified","additions":12,"deletions":3},
            {"filename":"Cargo.lock","status":"modified","additions":1,"deletions":1}
        ]"#;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls/7/files"))
            .and(query_param("per_page", "100"))
            .and(header("authorization", "Bearer dummy-token"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let paths = client.list_changed_paths("octocat/hello", 7).await.unwrap();
        assert_eq!(paths, vec!["src/auth/login.rs", "Cargo.lock"]);
    }

    #[tokio::test]
    async fn list_changed_paths_empty_array_is_empty_vec() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls/7/files"))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let paths = client.list_changed_paths("octocat/hello", 7).await.unwrap();
        assert!(paths.is_empty());
    }

    #[tokio::test]
    async fn list_changed_paths_follows_pagination_across_two_pages() {
        let server = MockServer::start().await;
        let next_url = format!("{}/repos/octocat/hello/pulls/7/files?page=2", server.uri());

        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls/7/files"))
            .and(query_param("page", "2"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"[{"filename":"README.md"}]"#),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/pulls/7/files"))
            .and(query_param("per_page", "100"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Link", format!(r#"<{next_url}>; rel="next""#).as_str())
                    .set_body_string(
                        r#"[{"filename":"src/auth/login.rs"},{"filename":"Cargo.lock"}]"#,
                    ),
            )
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let paths = client.list_changed_paths("octocat/hello", 7).await.unwrap();
        assert_eq!(
            paths,
            vec!["src/auth/login.rs", "Cargo.lock", "README.md"],
            "should concatenate both pages in order"
        );
    }

    // ---- enrichment: test_summary ----

    #[tokio::test]
    async fn test_summary_failing_when_a_check_fails() {
        let server = MockServer::start().await;
        let check_runs = r#"{
            "total_count": 2,
            "check_runs": [
                {"name":"build","status":"completed","conclusion":"success"},
                {"name":"test","status":"completed","conclusion":"failure"}
            ]
        }"#;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/commits/deadbeef/check-runs"))
            .and(header("authorization", "Bearer dummy-token"))
            .respond_with(ResponseTemplate::new(200).set_body_string(check_runs))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/commits/deadbeef/status"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"state":"failure"}"#))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let summary = client
            .test_summary("octocat/hello", "deadbeef")
            .await
            .unwrap();
        assert_eq!(summary.state, domain::TestState::Failing);
        assert!(summary.failed >= 1);
    }

    #[tokio::test]
    async fn test_summary_passing_when_all_success() {
        let server = MockServer::start().await;
        let check_runs = r#"{
            "total_count": 2,
            "check_runs": [
                {"name":"build","status":"completed","conclusion":"success"},
                {"name":"test","status":"completed","conclusion":"success"}
            ]
        }"#;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/commits/cafe/check-runs"))
            .respond_with(ResponseTemplate::new(200).set_body_string(check_runs))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/commits/cafe/status"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"state":"success"}"#))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let summary = client.test_summary("octocat/hello", "cafe").await.unwrap();
        assert_eq!(summary.state, domain::TestState::Passing);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
    }

    // ---- Actions timing: list_workflow_runs ----

    fn workflow_runs_body() -> &'static str {
        // One fast (~30s), one slow (~1200s), one in-progress (no conclusion, missing updated_at),
        // one failed (~45s). Each carries extra fields the DTO must tolerate.
        r#"{
            "total_count": 4,
            "workflow_runs": [
                {"id":1,"name":"CI","status":"completed","conclusion":"success",
                 "run_started_at":"2026-06-15T00:00:00Z","updated_at":"2026-06-15T00:00:30Z",
                 "event":"push"},
                {"id":2,"name":"Release","status":"completed","conclusion":"success",
                 "run_started_at":"2026-06-15T01:00:00Z","updated_at":"2026-06-15T01:20:00Z"},
                {"id":3,"name":"CI","status":"in_progress","conclusion":null,
                 "run_started_at":"2026-06-15T02:00:00Z"},
                {"id":4,"name":"Nightly","status":"completed","conclusion":"failure",
                 "run_started_at":"2026-06-15T03:00:00Z","updated_at":"2026-06-15T03:00:45Z"}
            ]
        }"#
    }

    #[tokio::test]
    async fn list_workflow_runs_computes_durations_and_sends_auth() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/actions/runs"))
            .and(query_param("per_page", "50"))
            .and(header("authorization", "Bearer dummy-token"))
            .and(header("user-agent", "alurtmee"))
            .respond_with(ResponseTemplate::new(200).set_body_string(workflow_runs_body()))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let runs = client.list_workflow_runs("octocat/hello").await.unwrap();
        assert_eq!(runs.len(), 4);

        // repo is the argument, workflow is the run name.
        assert!(runs.iter().all(|r| r.repo == "octocat/hello"));

        let fast = &runs[0];
        assert_eq!(fast.id, 1);
        assert_eq!(fast.workflow, "CI");
        assert_eq!(fast.conclusion.as_deref(), Some("success"));
        assert_eq!(fast.duration_secs, 30);

        let slow = &runs[1];
        assert_eq!(slow.workflow, "Release");
        assert_eq!(slow.duration_secs, 1200);

        let in_progress = &runs[2];
        assert_eq!(in_progress.conclusion, None);
        assert_eq!(in_progress.duration_secs, 0, "missing updated_at → 0");

        let failed = &runs[3];
        assert_eq!(failed.conclusion.as_deref(), Some("failure"));
        assert_eq!(failed.duration_secs, 45);
    }

    #[tokio::test]
    async fn list_workflow_runs_empty_envelope_is_empty_vec() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"total_count":0,"workflow_runs":[]}"#),
            )
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let runs = client.list_workflow_runs("octocat/hello").await.unwrap();
        assert!(runs.is_empty());
    }

    #[tokio::test]
    async fn list_workflow_runs_maps_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/actions/runs"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "bad-token").unwrap();
        let err = client
            .list_workflow_runs("octocat/hello")
            .await
            .unwrap_err();
        assert!(matches!(err, GhError::Unauthorized));
    }

    #[test]
    fn map_workflow_run_clamps_negative_duration_to_zero() {
        // updated_at before run_started_at (clock skew) must not underflow.
        let w = WireWorkflowRun {
            id: 9,
            name: "CI".to_string(),
            status: "completed".to_string(),
            conclusion: Some("success".to_string()),
            run_started_at: Some("2026-06-15T00:00:30Z".to_string()),
            updated_at: Some("2026-06-15T00:00:00Z".to_string()),
        };
        let run = map_workflow_run(w, "octocat/hello");
        assert_eq!(run.duration_secs, 0);
        assert_eq!(run.repo, "octocat/hello");
    }

    #[test]
    fn map_workflow_run_zero_when_a_timestamp_is_missing() {
        let w = WireWorkflowRun {
            id: 10,
            name: "CI".to_string(),
            status: "queued".to_string(),
            conclusion: None,
            run_started_at: None,
            updated_at: Some("2026-06-15T00:00:00Z".to_string()),
        };
        let run = map_workflow_run(w, "octocat/hello");
        assert_eq!(run.duration_secs, 0);
        assert_eq!(run.conclusion, None);
    }

    #[tokio::test]
    async fn test_summary_pending_when_no_checks_and_status_pending() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/commits/beef/check-runs"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"{"total_count":0,"check_runs":[]}"#),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello/commits/beef/status"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"state":"pending"}"#))
            .mount(&server)
            .await;

        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let summary = client.test_summary("octocat/hello", "beef").await.unwrap();
        assert_eq!(summary.state, domain::TestState::Pending);
    }
}
