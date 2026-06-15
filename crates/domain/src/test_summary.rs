use serde::{Deserialize, Serialize};

use crate::check_run::CheckRun;
use crate::test_state::TestState;

/// A reconciled CI verdict for a PR: per-outcome counts plus an overall [`TestState`] badge.
///
/// A repository may report results via the Checks API (`check-runs`), the legacy combined commit
/// status, or both (PHASE-3 §9). [`TestSummary::reconcile`] folds both sources into one verdict:
/// counts come from the check-runs, while the legacy combined status can only *raise* severity
/// (a failing status fails the PR even if no check-run did), so neither source is silently lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TestSummary {
    /// Completed, non-failing check-runs.
    pub passed: u32,
    /// Failing check-runs (`failure`, `timed_out`, `startup_failure`, `action_required`).
    pub failed: u32,
    /// Check-runs still queued/in-progress (or completed without a conclusion).
    pub pending: u32,
    /// The overall badge state.
    pub state: TestState,
}

impl TestSummary {
    /// Reconcile a set of check-runs and an optional legacy combined status (`success` | `failure`
    /// | `pending`) into a single verdict. Counts reflect the check-runs; the combined status can
    /// escalate the overall `state` (e.g. a failing legacy status fails the PR).
    pub fn reconcile(runs: &[CheckRun], combined_status: Option<&str>) -> Self {
        let mut passed = 0;
        let mut failed = 0;
        let mut pending = 0;

        for run in runs {
            if run.status != "completed" {
                pending += 1;
                continue;
            }
            match run.conclusion.as_deref() {
                Some("success") => passed += 1,
                Some("failure")
                | Some("timed_out")
                | Some("startup_failure")
                | Some("action_required") => failed += 1,
                None => pending += 1,
                // neutral / skipped / cancelled: completed but non-failing.
                _ => passed += 1,
            }
        }

        let state = if failed > 0 || combined_status == Some("failure") {
            TestState::Failing
        } else if pending > 0 || combined_status == Some("pending") {
            TestState::Pending
        } else if passed > 0 || combined_status == Some("success") {
            TestState::Passing
        } else {
            TestState::None
        };

        Self {
            passed,
            failed,
            pending,
            state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(status: &str, conclusion: Option<&str>) -> CheckRun {
        CheckRun {
            name: "ci".to_string(),
            status: status.to_string(),
            conclusion: conclusion.map(str::to_string),
        }
    }

    #[test]
    fn empty_is_none() {
        assert_eq!(TestSummary::reconcile(&[], None).state, TestState::None);
    }

    #[test]
    fn all_success_is_passing() {
        let runs = [
            run("completed", Some("success")),
            run("completed", Some("success")),
        ];
        let summary = TestSummary::reconcile(&runs, None);
        assert_eq!(summary.state, TestState::Passing);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
    }

    #[test]
    fn any_failure_is_failing() {
        let runs = [
            run("completed", Some("success")),
            run("completed", Some("failure")),
        ];
        let summary = TestSummary::reconcile(&runs, None);
        assert_eq!(summary.state, TestState::Failing);
        assert_eq!(summary.failed, 1);
    }

    #[test]
    fn incomplete_run_is_pending() {
        let runs = [run("in_progress", None), run("completed", Some("success"))];
        let summary = TestSummary::reconcile(&runs, None);
        assert_eq!(summary.state, TestState::Pending);
        assert_eq!(summary.pending, 1);
    }

    #[test]
    fn neutral_and_skipped_count_as_non_failing() {
        let runs = [
            run("completed", Some("neutral")),
            run("completed", Some("skipped")),
        ];
        let summary = TestSummary::reconcile(&runs, None);
        assert_eq!(summary.state, TestState::Passing);
        assert_eq!(summary.passed, 2);
    }

    #[test]
    fn combined_failure_escalates_even_when_checks_pass() {
        let runs = [run("completed", Some("success"))];
        let summary = TestSummary::reconcile(&runs, Some("failure"));
        assert_eq!(
            summary.state,
            TestState::Failing,
            "legacy failing status fails the PR"
        );
    }

    #[test]
    fn combined_status_alone_drives_state_when_no_check_runs() {
        assert_eq!(
            TestSummary::reconcile(&[], Some("success")).state,
            TestState::Passing
        );
        assert_eq!(
            TestSummary::reconcile(&[], Some("pending")).state,
            TestState::Pending
        );
        assert_eq!(
            TestSummary::reconcile(&[], Some("failure")).state,
            TestState::Failing
        );
    }
}
