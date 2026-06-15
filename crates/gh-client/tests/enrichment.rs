//! End-to-end (mock-first) integration test for the enrichment lane: reviews + comments +
//! test_summary for a single PR, served from documented GitHub JSON fixtures against a `wiremock`
//! server. No live network, no real token.

use gh_client::GhClient;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PR_REVIEWS: &str = include_str!("fixtures/pr_reviews.json");
const PR_ISSUE_COMMENTS: &str = include_str!("fixtures/pr_issue_comments.json");
const PR_REVIEW_COMMENTS: &str = include_str!("fixtures/pr_review_comments.json");
const COMMIT_CHECK_RUNS: &str = include_str!("fixtures/commit_check_runs.json");
const COMMIT_STATUS: &str = include_str!("fixtures/commit_status.json");

#[tokio::test]
async fn enrich_single_pr_end_to_end() {
    let server = MockServer::start().await;
    let repo = "octocat/hello";
    let number = 7;
    let head_sha = "deadbeefcafe";

    Mock::given(method("GET"))
        .and(path(format!("/repos/{repo}/pulls/{number}/reviews")))
        .and(header("authorization", "Bearer integration-token"))
        .and(header("user-agent", "alurtmee"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PR_REVIEWS))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/repos/{repo}/issues/{number}/comments")))
        .respond_with(ResponseTemplate::new(200).set_body_string(PR_ISSUE_COMMENTS))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/repos/{repo}/pulls/{number}/comments")))
        .respond_with(ResponseTemplate::new(200).set_body_string(PR_REVIEW_COMMENTS))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/repos/{repo}/commits/{head_sha}/check-runs")))
        .respond_with(ResponseTemplate::new(200).set_body_string(COMMIT_CHECK_RUNS))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/repos/{repo}/commits/{head_sha}/status")))
        .respond_with(ResponseTemplate::new(200).set_body_string(COMMIT_STATUS))
        .mount(&server)
        .await;

    let client = GhClient::new(server.uri(), "integration-token").unwrap();

    // Reviews.
    let reviews = client.list_reviews(repo, number).await.unwrap();
    assert_eq!(reviews.len(), 2);
    assert_eq!(reviews[0].author, "alice");
    assert_eq!(reviews[0].state, "APPROVED");
    assert_eq!(reviews[1].state, "CHANGES_REQUESTED");

    // Comments: issue stream first, then review stream, kind attributed, authors preserved.
    let comments = client.list_comments(repo, number).await.unwrap();
    assert_eq!(comments.len(), 3);
    assert_eq!(comments[0].kind, domain::CommentKind::Issue);
    assert_eq!(comments[0].author, "alice");
    assert_eq!(comments[1].kind, domain::CommentKind::Issue);
    assert_eq!(comments[1].author, "carol");
    assert_eq!(comments[2].kind, domain::CommentKind::Review);
    assert_eq!(comments[2].author, "bob");

    // Test summary: check-runs (one success, one failure) + failing combined status -> Failing.
    let summary = client.test_summary(repo, head_sha).await.unwrap();
    assert_eq!(summary.state, domain::TestState::Failing);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 1);
}
