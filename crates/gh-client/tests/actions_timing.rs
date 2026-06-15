//! End-to-end (mock-first) integration test for the Actions-timing lane: a repository's recent
//! workflow runs, served from a documented GitHub JSON fixture against a `wiremock` server. No live
//! network, no real token. Asserts durations are derived from the run timestamps and that an
//! in-progress run surfaces as `conclusion: None` with a zero duration.

use gh_client::GhClient;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const WORKFLOW_RUNS: &str = include_str!("fixtures/workflow_runs.json");

#[tokio::test]
async fn list_workflow_runs_end_to_end() {
    let server = MockServer::start().await;
    let repo = "octocat/hello";

    Mock::given(method("GET"))
        .and(path(format!("/repos/{repo}/actions/runs")))
        .and(query_param("per_page", "50"))
        .and(header("authorization", "Bearer integration-token"))
        .and(header("user-agent", "alurtmee"))
        .respond_with(ResponseTemplate::new(200).set_body_string(WORKFLOW_RUNS))
        .mount(&server)
        .await;

    let client = GhClient::new(server.uri(), "integration-token").unwrap();
    let runs = client.list_workflow_runs(repo).await.unwrap();

    assert_eq!(runs.len(), 4);
    assert!(runs.iter().all(|r| r.repo == repo));

    // Fast run: 30s, named "CI".
    assert_eq!(runs[0].id, 101);
    assert_eq!(runs[0].workflow, "CI");
    assert_eq!(runs[0].duration_secs, 30);
    assert_eq!(runs[0].conclusion.as_deref(), Some("success"));

    // Slow run: 1200s.
    assert_eq!(runs[1].workflow, "Release");
    assert_eq!(runs[1].duration_secs, 1200);

    // In-progress run: no conclusion, zero duration (no updated_at).
    assert_eq!(runs[2].conclusion, None);
    assert_eq!(runs[2].duration_secs, 0);

    // Failed run.
    assert_eq!(runs[3].conclusion.as_deref(), Some("failure"));
    assert_eq!(runs[3].duration_secs, 45);
}
