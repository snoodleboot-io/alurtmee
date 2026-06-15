//! De-duplicating dispatch of [`domain::CiAlert`]s to a [`Notifier`] backend.
//!
//! The poller emits a [`domain::CiAlert`] for each newly-seen CI condition. The
//! [`NotificationDispatcher`] is the app-side guard that turns at-most-one of those into a desktop
//! notification per `(run_id, kind)` pair, so a long-lived run that is observed across several
//! polls only ever notifies the user once.

use std::collections::HashSet;

use domain::{CiAlert, CiAlertKind};

use crate::notifier::Notifier;

/// Dispatches CI alerts to a [`Notifier`], suppressing duplicates by [`CiAlert::dedupe_key`].
///
/// Generic over the backend `N` so the Linux [`crate::xdg_notifier::XdgNotifier`] (or a test fake,
/// or a future macOS/Windows backend) can be plugged in without changing this logic.
// integrated by the orchestrator in the poll-event handler
#[allow(dead_code)]
pub struct NotificationDispatcher<N: Notifier> {
    notifier: N,
    /// Keys `(run_id, kind)` already dispatched, so each alert notifies at most once.
    seen: HashSet<(u64, CiAlertKind)>,
}

// integrated by the orchestrator in the poll-event handler
#[allow(dead_code)]
impl<N: Notifier> NotificationDispatcher<N> {
    /// Build a dispatcher over the given notification backend.
    pub fn new(notifier: N) -> Self {
        Self {
            notifier,
            seen: HashSet::new(),
        }
    }

    /// Dispatch `alert` as a desktop notification unless an alert with the same
    /// `(run_id, kind)` has already been dispatched.
    ///
    /// Returns `true` if this call dispatched (the alert was new), `false` if it was a duplicate
    /// and was skipped. A backend delivery failure is logged but the alert is still marked as seen
    /// — we deliberately do not retry, to avoid spamming the user on a flaky notification service.
    pub fn dispatch(&mut self, alert: &CiAlert) -> bool {
        let key = alert.dedupe_key();
        if !self.seen.insert(key) {
            return false;
        }

        let summary = match alert.kind {
            CiAlertKind::SlowCi => "Alurtmee — slow CI",
            CiAlertKind::Failure => "Alurtmee — CI failure",
        };
        // No token / secret / PII in the body — only repo, workflow, and the human reason string.
        let body = format!("{} · {}: {}", alert.repo, alert.workflow, alert.reason);

        if let Err(err) = self.notifier.notify(summary, &body) {
            tracing::warn!("failed to deliver CI notification: {err}");
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// A test double that records each `(summary, body)` it is asked to deliver, with no real
    /// D-Bus traffic — so the suite is hermetic and passes in CI.
    struct FakeNotifier {
        calls: RefCell<Vec<(String, String)>>,
    }

    impl FakeNotifier {
        fn new() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Notifier for FakeNotifier {
        fn notify(&self, summary: &str, body: &str) -> Result<(), crate::notifier::NotifyError> {
            self.calls
                .borrow_mut()
                .push((summary.to_string(), body.to_string()));
            Ok(())
        }
    }

    fn alert(run_id: u64, kind: CiAlertKind, reason: &str) -> CiAlert {
        CiAlert {
            repo: "acme/widgets".to_string(),
            workflow: "ci.yml".to_string(),
            run_id,
            kind,
            reason: reason.to_string(),
        }
    }

    #[test]
    fn dispatches_a_new_alert_once_and_skips_a_duplicate() {
        let mut dispatcher = NotificationDispatcher::new(FakeNotifier::new());
        let a = alert(1, CiAlertKind::Failure, "exit code 1");

        assert!(dispatcher.dispatch(&a), "first dispatch should notify");
        assert!(
            !dispatcher.dispatch(&a),
            "second dispatch of same (run_id, kind) should be skipped"
        );

        assert_eq!(
            dispatcher.notifier.calls.borrow().len(),
            1,
            "duplicate must not produce a second notification"
        );
    }

    #[test]
    fn distinct_run_ids_each_notify() {
        let mut dispatcher = NotificationDispatcher::new(FakeNotifier::new());

        assert!(dispatcher.dispatch(&alert(1, CiAlertKind::Failure, "boom")));
        assert!(dispatcher.dispatch(&alert(2, CiAlertKind::Failure, "boom")));

        assert_eq!(dispatcher.notifier.calls.borrow().len(), 2);
    }

    #[test]
    fn same_run_different_kind_each_notify() {
        let mut dispatcher = NotificationDispatcher::new(FakeNotifier::new());

        assert!(dispatcher.dispatch(&alert(7, CiAlertKind::SlowCi, "ran 12m")));
        assert!(dispatcher.dispatch(&alert(7, CiAlertKind::Failure, "exit 1")));

        assert_eq!(
            dispatcher.notifier.calls.borrow().len(),
            2,
            "dedupe key includes kind, so both should notify"
        );
    }

    #[test]
    fn body_carries_repo_workflow_reason_verbatim_and_no_secret() {
        let mut dispatcher = NotificationDispatcher::new(FakeNotifier::new());
        let reason = "build failed: 3 tests failing in module auth";
        dispatcher.dispatch(&alert(42, CiAlertKind::Failure, reason));

        let calls = dispatcher.notifier.calls.borrow();
        let (summary, body) = &calls[0];

        assert_eq!(summary, "Alurtmee — CI failure");
        assert!(body.contains("acme/widgets"), "body must name the repo");
        assert!(body.contains("ci.yml"), "body must name the workflow");
        assert!(
            body.contains(reason),
            "the human reason string must pass through verbatim"
        );
        // Notification hygiene: nothing token-like is constructed from the alert. The body is built
        // only from repo/workflow/reason, so a token sentinel could never appear.
        assert!(
            !body.contains("ghp_") && !body.contains("github_pat_"),
            "body must never contain a token-like string"
        );
    }

    #[test]
    fn slow_ci_uses_the_slow_summary() {
        let mut dispatcher = NotificationDispatcher::new(FakeNotifier::new());
        dispatcher.dispatch(&alert(9, CiAlertKind::SlowCi, "ran 12m vs 3m baseline"));

        assert_eq!(
            dispatcher.notifier.calls.borrow()[0].0,
            "Alurtmee — slow CI"
        );
    }
}
