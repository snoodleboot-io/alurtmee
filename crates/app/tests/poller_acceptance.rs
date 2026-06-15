//! Phase 2 acceptance (ATDD) — conditional polling, against a mock GitHub server (R2a, no PAT).
//!
//! Drives the real `gh-client` → `poller` → `store` chain and asserts the observable outcomes that
//! define the phase: open PRs appear; a re-poll with the persisted ETag returns **304** and leaves
//! state untouched; a **restart** (reopening the store) still sends the persisted ETag and 304s; a
//! changed body refreshes via a diff. The *live* proof that a real GitHub 304 leaves
//! `X-RateLimit-Remaining` untouched is DEFERRED to the §10 pass (MASTER R2a).

use std::time::Duration;

use domain::{ChangeEvent, PollCadence};
use gh_client::GhClient;
use poller::Poller;
use store::Store;
use wiremock::matchers::{header, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DUMMY_TOKEN: &str = "ghp_dummy_token_not_a_real_pat";

fn cadence() -> PollCadence {
    PollCadence::new(Duration::from_millis(20), Duration::from_millis(100))
}

/// Mount empty/default responses for every enrichment endpoint so changed PRs enrich cleanly.
async fn mount_enrichment_defaults(server: &MockServer) {
    for suffix in [
        r"/reviews$",
        r"/pulls/\d+/comments$",
        r"/issues/\d+/comments$",
    ] {
        Mock::given(method("GET"))
            .and(path_regex(suffix))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
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

/// Count the change-detection events (added/updated/removed), ignoring enrichment events.
fn diff_event_count(events: &[ChangeEvent]) -> usize {
    events
        .iter()
        .filter(|e| !matches!(e, ChangeEvent::Enriched(_)))
        .count()
}

fn body_two_prs() -> &'static str {
    r#"[
        {"number":1,"title":"first","user":{"login":"octocat"},"draft":false,"updated_at":"t1","html_url":"https://github.com/o/r/pull/1"},
        {"number":2,"title":"second","user":{"login":"hubot"},"draft":true,"updated_at":"t1","html_url":"https://github.com/o/r/pull/2"}
    ]"#
}

fn unique_db_path(tag: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "alurtmee_poll_{tag}_{}_{seq}.sqlite",
        std::process::id()
    ));
    path.to_str().expect("utf-8 temp path").to_string()
}

#[tokio::test]
async fn prs_appear_then_304_then_survive_restart() {
    let server = MockServer::start().await;
    // Conditional re-poll (carrying the ETag) gets a 304; the first (etag-less) poll gets 200+body.
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304).insert_header("ETag", "\"v1\""))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"v1\"")
                .set_body_string(body_two_prs()),
        )
        .with_priority(5)
        .mount(&server)
        .await;
    mount_enrichment_defaults(&server).await;

    let db_path = unique_db_path("restart");
    let repos = vec!["o/r".to_string()];

    // 1. First poll: both PRs appear and are cached.
    {
        let client = GhClient::new(server.uri(), DUMMY_TOKEN).unwrap();
        let store = Store::open(&db_path).unwrap();
        let mut poller = Poller::new(client, store, cadence());

        let first = poller.poll_once(&repos).await.unwrap();
        assert!(first.changed);
        assert_eq!(diff_event_count(&first.events), 2, "two PRs appear");

        // 2. Immediate re-poll sends If-None-Match → 304 → no change.
        let second = poller.poll_once(&repos).await.unwrap();
        assert!(!second.changed, "304 is not a change");
        assert!(second.events.is_empty());
    } // poller + its store dropped → simulates application exit

    // 3. Restart: a brand-new store at the same path still holds the cached PRs and the ETag, so
    //    the first poll after restart sends the persisted ETag and gets a 304 (free refresh).
    let client = GhClient::new(server.uri(), DUMMY_TOKEN).unwrap();
    let store = Store::open(&db_path).unwrap();
    assert_eq!(
        store.load_repo_prs("o/r").unwrap().len(),
        2,
        "cache persisted across restart"
    );
    let mut poller = Poller::new(client, store, cadence());

    let after_restart = poller.poll_once(&repos).await.unwrap();
    assert!(
        !after_restart.changed,
        "persisted ETag yields a 304 on restart"
    );
    assert!(after_restart.events.is_empty());

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn changed_body_refreshes_the_list() {
    let server = MockServer::start().await;
    // Server returns a changed set (PR #1 advanced, #2 gone, #3 new) with a new ETag.
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls"))
        .respond_with(
            ResponseTemplate::new(200).insert_header("ETag", "\"v2\"").set_body_string(
                r#"[
                    {"number":1,"title":"first","user":{"login":"octocat"},"draft":false,"updated_at":"t2","html_url":"https://github.com/o/r/pull/1"},
                    {"number":3,"title":"third","user":{"login":"octocat"},"draft":false,"updated_at":"t1","html_url":"https://github.com/o/r/pull/3"}
                ]"#,
            ),
        )
        .mount(&server)
        .await;
    mount_enrichment_defaults(&server).await;

    let db_path = unique_db_path("refresh");
    let repos = vec!["o/r".to_string()];

    // Seed the cache with the prior set (two PRs at t1).
    let mut store = Store::open(&db_path).unwrap();
    store.cache_repo_prs("o/r", &poller_store_seed()).unwrap();
    drop(store);

    let client = GhClient::new(server.uri(), DUMMY_TOKEN).unwrap();
    let store = Store::open(&db_path).unwrap();
    let mut poller = Poller::new(client, store, cadence());

    let outcome = poller.poll_once(&repos).await.unwrap();
    assert!(outcome.changed);
    // #1 updated, #3 added, #2 removed (enrichment events excluded from this count).
    assert_eq!(diff_event_count(&outcome.events), 3);

    let _ = std::fs::remove_file(&db_path);
}

/// The prior cached PR set used to seed the "changed body" scenario (two PRs at `t1`).
fn poller_store_seed() -> Vec<domain::PullRequest> {
    use domain::{PrId, PullRequest};
    vec![
        PullRequest {
            id: PrId::new("o/r", 1),
            title: "first".to_string(),
            author: "octocat".to_string(),
            draft: false,
            updated_at: "t1".to_string(),
            url: "https://github.com/o/r/pull/1".to_string(),
            head_sha: String::new(),
        },
        PullRequest {
            id: PrId::new("o/r", 2),
            title: "second".to_string(),
            author: "hubot".to_string(),
            draft: true,
            updated_at: "t1".to_string(),
            url: "https://github.com/o/r/pull/2".to_string(),
            head_sha: String::new(),
        },
    ]
}
