//! Phase 1 acceptance (ATDD) — Auth + Scope, against a mock GitHub server (R2a, no live PAT).
//!
//! These scenarios drive the *real* collaborators end to end — `gh-client` (against a `wiremock`
//! GitHub server replaying fixtures), the `domain` types, and the persistence `Store` — and assert
//! the observable outcomes the user cares about. The Iced event loop itself is exercised separately
//! by the headless window smoke test; here we assert state, which is where the acceptance criteria
//! live: *validate a token, list orgs & repos, select a subset, and have it survive a restart.*
//!
//! Live `GET /user` verification against `api.github.com` is DEFERRED to the §10 Integration
//! Verification pass when a PAT is supplied (MASTER R2a).

use domain::RepoSelection;
use gh_client::{GhClient, GhError};
use store::Store;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DUMMY_TOKEN: &str = "ghp_dummy_token_not_a_real_pat";

/// Stand up a mock GitHub server serving valid `/user`, `/user/orgs`, and `/user/repos` fixtures.
async fn mock_github() -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 42,
            "login": "octocat",
            "type": "User",
            "name": "The Octocat"
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user/orgs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 100, "login": "acme" }
        ])))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "id": 1, "name": "hello", "full_name": "octocat/hello",
                "private": false, "owner": { "login": "octocat", "id": 42, "type": "User" }
            },
            {
                "id": 2, "name": "secret", "full_name": "acme/secret",
                "private": true, "owner": { "login": "acme", "id": 100, "type": "Organization" }
            }
        ])))
        .mount(&server)
        .await;

    server
}

#[tokio::test]
async fn valid_token_lists_orgs_and_repos_and_selection_survives_restart() {
    let server = mock_github().await;
    let client = GhClient::new(server.uri(), DUMMY_TOKEN).expect("build client");

    // 1. Validate the token → authenticated identity.
    let user = client
        .validate()
        .await
        .expect("validation succeeds against fixture");
    assert_eq!(user.login, "octocat");

    // 2. List orgs and repos → the picker is populated.
    let orgs = client.list_orgs().await.expect("list orgs");
    let repos = client.list_user_repos().await.expect("list repos");
    assert_eq!(orgs.len(), 1, "one org from fixture");
    assert_eq!(repos.len(), 2, "two repos from fixture");
    assert!(repos.iter().any(|r| r.full_name == "octocat/hello"));
    assert!(repos
        .iter()
        .any(|r| r.full_name == "acme/secret" && r.private));

    // 3. The user selects a subset and it is persisted to disk.
    let db_path = unique_db_path();
    {
        let store = Store::open(&db_path).expect("open store");
        let mut selection = RepoSelection::new();
        selection.insert("octocat/hello");
        store.save_selection(&selection).expect("persist selection");
    } // store dropped → simulates application exit

    // 4. Restart: a fresh store at the same path restores exactly the selected subset.
    let restored = {
        let store = Store::open(&db_path).expect("reopen store");
        store.load_selection().expect("load selection")
    };
    assert!(restored.contains("octocat/hello"));
    assert!(!restored.contains("acme/secret"));
    assert_eq!(restored.len(), 1);

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn invalid_token_surfaces_a_clear_error_without_panicking() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let client = GhClient::new(server.uri(), "bad-token").expect("build client");
    let result = client.validate().await;

    assert!(
        matches!(result, Err(GhError::Unauthorized)),
        "401 must map to a clear Unauthorized error, got {result:?}"
    );
}

/// A unique on-disk SQLite path for restart-persistence tests (avoids cross-test collisions).
fn unique_db_path() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "alurtmee_acceptance_{}_{seq}.sqlite",
        std::process::id()
    ));
    path.to_str().expect("utf-8 temp path").to_string()
}
