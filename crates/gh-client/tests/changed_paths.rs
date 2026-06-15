//! End-to-end (mock-first) integration test for the changed-files lane: the changed paths of a
//! single PR, served from a documented GitHub JSON fixture against a `wiremock` server. No live
//! network, no real token.

use gh_client::GhClient;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PR_FILES: &str = include_str!("fixtures/pr_files.json");

#[tokio::test]
async fn changed_paths_single_pr_end_to_end() {
    let server = MockServer::start().await;
    let repo = "octocat/hello";
    let number = 7;

    Mock::given(method("GET"))
        .and(path(format!("/repos/{repo}/pulls/{number}/files")))
        .and(query_param("per_page", "100"))
        .and(header("authorization", "Bearer integration-token"))
        .and(header("user-agent", "alurtmee"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PR_FILES))
        .mount(&server)
        .await;

    let client = GhClient::new(server.uri(), "integration-token").unwrap();
    let paths = client.list_changed_paths(repo, number).await.unwrap();

    assert_eq!(paths, vec!["src/auth/login.rs", "Cargo.lock"]);
}
