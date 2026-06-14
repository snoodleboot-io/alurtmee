//! End-to-end (mock-first) integration test: orgs + org repos against a `wiremock` server using
//! the documented GitHub JSON fixtures. No live network, no real token.

use gh_client::GhClient;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER_ORGS: &str = include_str!("fixtures/user_orgs.json");
const ORG_REPOS: &str = include_str!("fixtures/org_repos.json");

#[tokio::test]
async fn list_orgs_then_org_repos_end_to_end() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/user/orgs"))
        .and(header("authorization", "Bearer integration-token"))
        .and(header("user-agent", "alurtmee"))
        .respond_with(ResponseTemplate::new(200).set_body_string(USER_ORGS))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/orgs/acme-engineering/repos"))
        .and(header("user-agent", "alurtmee"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ORG_REPOS))
        .mount(&server)
        .await;

    let client = GhClient::new(server.uri(), "integration-token").unwrap();

    let orgs = client.list_orgs().await.unwrap();
    assert_eq!(orgs.len(), 2);
    assert!(orgs.iter().any(|o| o.login == "acme-engineering"));
    assert!(orgs.iter().any(|o| o.login == "octo-labs"));

    let repos = client.list_org_repos("acme-engineering").await.unwrap();
    assert_eq!(repos.len(), 2);

    let platform = repos
        .iter()
        .find(|r| r.full_name == "acme-engineering/platform")
        .expect("platform repo present");
    assert_eq!(platform.owner, "acme-engineering");
    assert_eq!(platform.name, "platform");
    assert!(platform.private);

    let docs = repos
        .iter()
        .find(|r| r.full_name == "acme-engineering/docs-site")
        .expect("docs-site repo present");
    assert!(!docs.private);
}
