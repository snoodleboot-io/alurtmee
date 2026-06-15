//! Phase 5 acceptance (ATDD) — CI/CD timing, against a mock GitHub server (R2a, no PAT).
//!
//! Drives the real gh-client → poller → store → domain-baseline chain: a failed Actions run raises
//! a Failure alert and an over-threshold run raises a SlowCi alert, each exactly once (de-duped at
//! the source across cycles). The live desktop-notification path is verified separately as a unit
//! test in the app crate. Live Actions fetch is DEFERRED to §10.

use std::time::Duration;

use domain::{ChangeEvent, CiAlert, CiAlertKind, PollCadence};
use gh_client::GhClient;
use poller::Poller;
use store::Store;
use wiremock::matchers::{method, path};
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
        "alurtmee_ci_{tag}_{}_{seq}.sqlite",
        std::process::id()
    ));
    path.to_str().expect("utf-8 temp path").to_string()
}

fn ci_alerts(events: &[ChangeEvent]) -> Vec<CiAlert> {
    events
        .iter()
        .filter_map(|e| match e {
            ChangeEvent::CiAlert(a) => Some(a.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn failed_and_slow_runs_alert_once_each() {
    let server = MockServer::start().await;
    // No open PRs (keeps the cycle to the CI path).
    Mock::given(method("GET"))
        .and(path("/repos/o/r/pulls"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"v1\"")
                .set_body_string("[]"),
        )
        .mount(&server)
        .await;
    // One failed run (120s) and one over-threshold success (1200s > 600s cold-start threshold).
    Mock::given(method("GET"))
        .and(path("/repos/o/r/actions/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"workflow_runs":[
                {"id":9001,"name":"CI","status":"completed","conclusion":"failure",
                 "run_started_at":"2026-06-15T00:00:00Z","updated_at":"2026-06-15T00:02:00Z"},
                {"id":9002,"name":"Integration","status":"completed","conclusion":"success",
                 "run_started_at":"2026-06-15T00:00:00Z","updated_at":"2026-06-15T00:20:00Z"}
            ]}"#,
        ))
        .mount(&server)
        .await;

    let db_path = unique_db_path("alerts");
    let client = GhClient::new(server.uri(), DUMMY_TOKEN).unwrap();
    let store = Store::open(&db_path).unwrap();
    let mut poller = Poller::new(client, store, cadence());

    let first = poller.poll_once(&["o/r".to_string()]).await.unwrap();
    let alerts = ci_alerts(&first.events);

    let failure = alerts
        .iter()
        .find(|a| a.kind == CiAlertKind::Failure)
        .expect("failure alert");
    assert_eq!(failure.run_id, 9001);
    assert_eq!(failure.workflow, "CI");

    let slow = alerts
        .iter()
        .find(|a| a.kind == CiAlertKind::SlowCi)
        .expect("slow alert");
    assert_eq!(slow.run_id, 9002);
    assert!(
        slow.reason.contains("1200s"),
        "reason names the duration: {}",
        slow.reason
    );

    // Second cycle: the same runs are already recorded → no new alerts (de-duped at the source).
    let second = poller.poll_once(&["o/r".to_string()]).await.unwrap();
    assert!(
        ci_alerts(&second.events).is_empty(),
        "no re-alerts for already-seen runs"
    );

    let _ = std::fs::remove_file(&db_path);
}
