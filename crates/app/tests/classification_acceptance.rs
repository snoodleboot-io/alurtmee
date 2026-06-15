//! Phase 4 acceptance (ATDD) — classification, against a mock GitHub server (R2a, no PAT).
//!
//! Drives the real gh-client → poller → store → domain-classifier chain. First, four PRs each
//! crafted to trigger a different layer (label, title/branch prefix, changed path, Dependabot)
//! classify to the expected category with the expected firing signal. Second, a user correction
//! overrides the heuristic and persists across a restart.
//!
//! The classifier logic is pure and fully exercised here; only the live `/pulls/{n}/files` + advisory
//! *fetch* is deferred to §10.

use std::collections::HashMap;
use std::time::Duration;

use domain::{CategoryKind, ChangeEvent, Classification, PollCadence};
use gh_client::GhClient;
use poller::Poller;
use store::Store;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DUMMY_TOKEN: &str = "ghp_dummy_token_not_a_real_pat";

fn cadence() -> PollCadence {
    PollCadence::new(Duration::from_millis(20), Duration::from_millis(100))
}

fn unique_db_path(tag: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "alurtmee_class_{tag}_{}_{seq}.sqlite",
        std::process::id()
    ));
    path.to_str().expect("utf-8 temp path").to_string()
}

/// Default-empty responses for enrichment + classification endpoints (so changed PRs process
/// cleanly). A more specific `/files` mock can be mounted at higher priority where needed.
async fn mount_defaults(server: &MockServer) {
    for suffix in [r"/reviews$", r"/comments$", r"/files$"] {
        Mock::given(method("GET"))
            .and(path_regex(suffix))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .with_priority(5)
            .mount(server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path_regex(r"/check-runs$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"check_runs":[]}"#))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"/status$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"state":"success"}"#))
        .mount(server)
        .await;
}

fn classifications(events: &[ChangeEvent]) -> HashMap<u64, Classification> {
    events
        .iter()
        .filter_map(|e| match e {
            ChangeEvent::Classified(c) => Some((c.id.number, c.clone())),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn four_signals_classify_with_expected_category_and_signal() {
    let server = MockServer::start().await;
    // #1 label:security, #2 prefix:feature, #3 changed-path security, #4 Dependabot.
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "\"v1\"").set_body_string(
            r#"[
              {"number":1,"title":"Update deps","user":{"login":"octocat","type":"User"},"draft":false,
               "updated_at":"t1","html_url":"u1","head":{"sha":"s1","ref":"patch-1"},"labels":[{"name":"security"}]},
              {"number":2,"title":"feat: add widget","user":{"login":"octocat","type":"User"},"draft":false,
               "updated_at":"t1","html_url":"u2","head":{"sha":"s2","ref":"feat/widget"},"labels":[]},
              {"number":3,"title":"refactor module","user":{"login":"octocat","type":"User"},"draft":false,
               "updated_at":"t1","html_url":"u3","head":{"sha":"s3","ref":"refactor/x"},"labels":[]},
              {"number":4,"title":"Bump serde from 1.0 to 1.1","user":{"login":"dependabot[bot]","type":"Bot"},"draft":false,
               "updated_at":"t1","html_url":"u4","head":{"sha":"s4","ref":"dependabot/serde"},"labels":[]}
            ]"#,
        ))
        .mount(&server)
        .await;
    mount_defaults(&server).await;
    // #3's changed paths touch a sensitive area → path signal.
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls/3/files"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"[{"filename":"src/auth/session.rs"}]"#),
        )
        .with_priority(1)
        .mount(&server)
        .await;

    let db_path = unique_db_path("signals");
    let client = GhClient::new(server.uri(), DUMMY_TOKEN).unwrap();
    let store = Store::open(&db_path).unwrap();
    let mut poller = Poller::new(client, store, cadence());

    let outcome = poller.poll_once(&["o/r".to_string()]).await.unwrap();
    let by_number = classifications(&outcome.events);
    assert_eq!(by_number.len(), 4, "every changed PR is classified");

    assert_eq!(by_number[&1].category.kind, CategoryKind::Security);
    assert_eq!(by_number[&1].category.signal, "label:security");

    assert_eq!(by_number[&2].category.kind, CategoryKind::Feature);
    assert_eq!(by_number[&2].category.signal, "prefix:feature");

    assert_eq!(by_number[&3].category.kind, CategoryKind::Security);
    assert_eq!(by_number[&3].category.signal, "path");

    assert_eq!(by_number[&4].category.kind, CategoryKind::Security);
    assert_eq!(by_number[&4].category.signal, "dependabot");
    // #4 is also tagged a bot.
    assert_eq!(by_number[&4].author_kind, domain::AuthorKind::Bot);

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn correction_overrides_classification_and_persists() {
    let server = MockServer::start().await;
    // A PR that would heuristically classify as Feature (prefix:feature).
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "\"v1\"").set_body_string(
            r#"[{"number":2,"title":"feat: add widget","user":{"login":"octocat","type":"User"},
                 "draft":false,"updated_at":"t1","html_url":"u2","head":{"sha":"s2","ref":"feat/widget"},"labels":[]}]"#,
        ))
        .mount(&server)
        .await;
    mount_defaults(&server).await;

    let db_path = unique_db_path("correction");
    let client = GhClient::new(server.uri(), DUMMY_TOKEN).unwrap();
    let store = Store::open(&db_path).unwrap();
    // The user has previously corrected this PR to Security (the PR is new → Added → classified).
    store
        .set_correction("o/r", 2, CategoryKind::Security)
        .unwrap();
    let mut poller = Poller::new(client, store, cadence());

    let outcome = poller.poll_once(&["o/r".to_string()]).await.unwrap();
    let by_number = classifications(&outcome.events);
    // The correction overrides the prefix heuristic.
    assert_eq!(by_number[&2].category.kind, CategoryKind::Security);
    assert_eq!(by_number[&2].category.signal, "correction");

    // And it survives a restart (fresh store at the same path still has it).
    drop(poller);
    let store = Store::open(&db_path).unwrap();
    assert_eq!(
        store.get_correction("o/r", 2).unwrap(),
        Some(CategoryKind::Security),
        "correction persisted across restart"
    );

    let _ = std::fs::remove_file(&db_path);
}
