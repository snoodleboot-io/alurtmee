//! End-to-end (mock-first) integration test for the conditional `list_open_prs` lane.
//!
//! Drives the realistic poller cycle against a `wiremock` server: a first poll (no `ETag`) returns
//! `200` with an `ETag`; the second poll replays that `ETag` as `If-None-Match` and GitHub answers
//! `304 Not Modified` for free (AD-1). No live network, no real token.

use gh_client::{GhClient, PrOutcome};
use wiremock::matchers::{header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REPO_PULLS: &str = include_str!("fixtures/repo_pulls.json");

#[tokio::test]
async fn poll_then_conditional_repoll_returns_200_then_304() {
    let server = MockServer::start().await;
    let repo = "acme-engineering/platform";

    // Second poll: carries `If-None-Match: "etag-1"` → 304 Not Modified, no body.
    Mock::given(method("GET"))
        .and(path("/repos/acme-engineering/platform/pulls"))
        .and(header("if-none-match", "\"etag-1\""))
        .respond_with(ResponseTemplate::new(304).insert_header("ETag", "\"etag-1\""))
        .mount(&server)
        .await;

    // First poll: no `If-None-Match` → 200 with the list + an ETag to persist.
    Mock::given(method("GET"))
        .and(path("/repos/acme-engineering/platform/pulls"))
        .and(query_param("state", "open"))
        .and(query_param("per_page", "100"))
        .and(header("authorization", "Bearer integration-token"))
        .and(header("user-agent", "alurtmee"))
        .and(matcher_no_conditional())
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"etag-1\"")
                .insert_header("x-ratelimit-limit", "5000")
                .insert_header("x-ratelimit-remaining", "4998")
                .insert_header("x-ratelimit-reset", "1700000000")
                .insert_header("x-poll-interval", "60")
                .set_body_string(REPO_PULLS),
        )
        .mount(&server)
        .await;

    let client = GhClient::new(server.uri(), "integration-token").unwrap();

    // First poll — no cached ETag.
    let first = client.list_open_prs(repo, None).await.unwrap();
    let etag = first.etag.clone().expect("first poll returns an ETag");
    assert_eq!(etag, "\"etag-1\"");
    match first.outcome {
        PrOutcome::Modified(prs) => {
            assert_eq!(prs.len(), 2);
            assert_eq!(prs[0].id.repo, repo);
            assert_eq!(prs[0].id.number, 101);
            assert_eq!(prs[0].author, "alice");
            assert!(!prs[0].draft);
            assert_eq!(prs[1].id.number, 102);
            assert!(prs[1].draft);
        }
        other => panic!("expected Modified on first poll, got {other:?}"),
    }
    let rl = first.rate_limit.expect("rate limit parsed");
    assert_eq!(rl.remaining, 4998);
    assert_eq!(
        first.poll_interval,
        Some(std::time::Duration::from_secs(60))
    );

    // Second poll — replay the ETag; nothing changed → 304.
    let second = client.list_open_prs(repo, Some(&etag)).await.unwrap();
    assert_eq!(second.outcome, PrOutcome::NotModified);
    assert_eq!(second.etag.as_deref(), Some("\"etag-1\""));
}

/// Helper matcher: the request must NOT carry an `If-None-Match` header (the first, unconditional
/// poll). Implemented by negating `header_exists`.
fn matcher_no_conditional() -> impl wiremock::Match {
    NotMatch(header_exists("if-none-match"))
}

struct NotMatch<M>(M);

impl<M: wiremock::Match> wiremock::Match for NotMatch<M> {
    fn matches(&self, request: &wiremock::Request) -> bool {
        !self.0.matches(request)
    }
}
