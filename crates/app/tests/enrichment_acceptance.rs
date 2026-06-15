//! Phase 3 acceptance (ATDD) — enrichment, against a mock GitHub server (R2a, no PAT).
//!
//! Two observable guarantees define the phase. First, a changed PR is enriched: reviews,
//! attributed comments (issue + review), and a reconciled test verdict appear and are persisted.
//! Second, enrichment fires ONLY on change — a 304 cycle makes zero enrichment requests (proven by
//! the mock's recorded request log).
//!
//! Live per-family verification against a real reviewed/commented/CI'd PR is DEFERRED to §10.

use std::time::Duration;

use domain::{ChangeEvent, CommentKind, PollCadence, PrId, TestState};
use gh_client::GhClient;
use poller::Poller;
use store::{EtagRecord, Store};
use wiremock::matchers::{header, method, path, path_regex};
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
        "alurtmee_enrich_{tag}_{}_{seq}.sqlite",
        std::process::id()
    ));
    path.to_str().expect("utf-8 temp path").to_string()
}

#[tokio::test]
async fn changed_pr_is_enriched_with_reviews_comments_and_test_verdict() {
    let server = MockServer::start().await;

    // One open PR with a head sha that keys the check-runs/status lookups.
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"v1\"")
                .set_body_string(
                    r#"[{"number":7,"title":"Add feature","user":{"login":"octocat"},"draft":false,
                 "updated_at":"t1","html_url":"https://github.com/o/r/pull/7",
                 "head":{"sha":"abc123"}}]"#,
                ),
        )
        .mount(&server)
        .await;
    // Two reviews.
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls/7/reviews"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"[{"user":{"login":"alice"},"state":"APPROVED","submitted_at":"t2"},
                {"user":{"login":"bob"},"state":"CHANGES_REQUESTED","submitted_at":"t3"}]"#,
        ))
        .mount(&server)
        .await;
    // One issue comment + one review comment → merged & attributed.
    Mock::given(method("GET"))
        .and(path("/repos/o/r/issues/7/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"[{"user":{"login":"carol"},"body":"looks good","created_at":"t4"}]"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls/7/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"[{"user":{"login":"dave"},"body":"nit: rename","created_at":"t5"}]"#,
        ))
        .mount(&server)
        .await;
    // Checks: one passing run + a success combined status → Passing.
    Mock::given(method("GET"))
        .and(path("/repos/o/r/commits/abc123/check-runs"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"check_runs":[{"name":"build","status":"completed","conclusion":"success"}]}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/o/r/commits/abc123/status"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"state":"success"}"#))
        .mount(&server)
        .await;

    let db_path = unique_db_path("detail");
    let client = GhClient::new(server.uri(), DUMMY_TOKEN).unwrap();
    let store = Store::open(&db_path).unwrap();
    let mut poller = Poller::new(client, store, cadence());

    let outcome = poller.poll_once(&["o/r".to_string()]).await.unwrap();

    // The poll yields the enrichment for the new PR.
    let enrichment = outcome
        .events
        .iter()
        .find_map(|e| match e {
            ChangeEvent::Enriched(en) => Some(en.clone()),
            _ => None,
        })
        .expect("an Enriched event for the changed PR");

    assert_eq!(enrichment.id, PrId::new("o/r", 7));
    assert_eq!(enrichment.reviews.len(), 2);
    assert_eq!(enrichment.reviews[0].author, "alice");
    assert_eq!(enrichment.reviews[0].state, "APPROVED");

    assert_eq!(enrichment.comments.len(), 2);
    // Issue comments come first, then review comments — attribution preserved.
    assert_eq!(enrichment.comments[0].kind, CommentKind::Issue);
    assert_eq!(enrichment.comments[0].author, "carol");
    assert_eq!(enrichment.comments[1].kind, CommentKind::Review);
    assert_eq!(enrichment.comments[1].author, "dave");

    assert_eq!(enrichment.tests.state, TestState::Passing);
    assert_eq!(enrichment.tests.passed, 1);

    // It was persisted: a fresh store at the same path reloads the same enrichment.
    drop(poller);
    let store = Store::open(&db_path).unwrap();
    let loaded = store
        .load_enrichment(&PrId::new("o/r", 7))
        .unwrap()
        .expect("enrichment persisted across restart");
    assert_eq!(loaded, enrichment);

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn no_change_cycle_makes_zero_enrichment_requests() {
    let server = MockServer::start().await;
    // The conditional poll returns 304 (nothing changed).
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304).insert_header("ETag", "\"v1\""))
        .mount(&server)
        .await;
    // Enrichment endpoints are mounted too — but must NEVER be hit on a no-change cycle.
    for suffix in [r"/reviews$", r"/comments$", r"/check-runs$", r"/status$"] {
        Mock::given(method("GET"))
            .and(path_regex(suffix))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(&server)
            .await;
    }

    let db_path = unique_db_path("nochange");
    let client = GhClient::new(server.uri(), DUMMY_TOKEN).unwrap();
    let mut store = Store::open(&db_path).unwrap();
    // Seed a persisted ETag + cached PR so the poll is a pure 304.
    store
        .set_etag(
            "pulls:o/r",
            &EtagRecord {
                etag: Some("\"v1\"".to_string()),
                last_modified: None,
            },
        )
        .unwrap();
    store.cache_repo_prs("o/r", &[]).unwrap();
    let mut poller = Poller::new(client, store, cadence());

    let outcome = poller.poll_once(&["o/r".to_string()]).await.unwrap();
    assert!(!outcome.changed);
    assert!(outcome.events.is_empty());

    // The recorded request log proves enrichment never fired.
    let requests = server
        .received_requests()
        .await
        .expect("request recording enabled");
    let enrich_hits = requests
        .iter()
        .filter(|r| {
            let p = r.url.path();
            p.contains("/reviews")
                || p.contains("/comments")
                || p.contains("/check-runs")
                || p.ends_with("/status")
        })
        .count();
    assert_eq!(
        enrich_hits, 0,
        "a 304 cycle must make zero enrichment requests"
    );

    let _ = std::fs::remove_file(&db_path);
}
