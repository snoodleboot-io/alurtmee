//! Unit-level tests that drive the `Poller` through its [`GhApi`] / [`PollStore`] ports with pure
//! in-memory fakes — no `wiremock` server, no SQLite, no token. This is the payoff of the DIP seam:
//! the poll cycle's behaviour (and its error propagation) can be asserted in microseconds against
//! hand-built doubles that the previous concrete-typed `Poller` made impossible.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use domain::{
    BotOverrides, CategoryKind, ChangeEvent, Comment, LabelMap, PollCadence, PrEnrichment, PrId,
    PullRequest, Review, TestSummary, WorkflowRun,
};
use gh_client::{GhError, OpenPrsResult, PrOutcome};
use poller::{GhApi, PollStore, Poller};
use store::{EtagRecord, StoreError};

fn sample_pr() -> PullRequest {
    PullRequest {
        id: PrId::new("o/r", 1),
        title: "first".to_string(),
        author: "octocat".to_string(),
        draft: false,
        updated_at: "t1".to_string(),
        url: "https://github.com/o/r/pull/1".to_string(),
        head_sha: "deadbeef".to_string(),
        author_type: "User".to_string(),
        head_ref: "feature".to_string(),
        labels: Vec::new(),
    }
}

fn cadence() -> PollCadence {
    PollCadence::new(Duration::from_millis(20), Duration::from_millis(100))
}

// ---- fake GitHub access ---------------------------------------------------

/// A `GhApi` that returns a fixed open-PR list and empty enrichment, or a forced error.
struct FakeGh {
    open_prs: Result<Vec<PullRequest>, GhError>,
}

impl FakeGh {
    fn modified(prs: Vec<PullRequest>) -> Self {
        Self { open_prs: Ok(prs) }
    }
    fn failing(err: GhError) -> Self {
        Self { open_prs: Err(err) }
    }
}

impl GhApi for FakeGh {
    async fn list_open_prs(
        &self,
        _repo: &str,
        _etag: Option<&str>,
    ) -> Result<OpenPrsResult, GhError> {
        match &self.open_prs {
            Ok(prs) => Ok(OpenPrsResult {
                outcome: PrOutcome::Modified(prs.clone()),
                etag: Some("\"v1\"".to_string()),
                rate_limit: None,
                poll_interval: None,
            }),
            // GhError isn't `Clone`; reconstruct the same shape for the error-path test.
            Err(GhError::Unauthorized) => Err(GhError::Unauthorized),
            Err(GhError::Http { status }) => Err(GhError::Http { status: *status }),
            Err(_) => Err(GhError::Unauthorized),
        }
    }

    async fn list_reviews(&self, _repo: &str, _number: u64) -> Result<Vec<Review>, GhError> {
        Ok(Vec::new())
    }

    async fn list_comments(&self, _repo: &str, _number: u64) -> Result<Vec<Comment>, GhError> {
        Ok(Vec::new())
    }

    async fn list_changed_paths(&self, _repo: &str, _number: u64) -> Result<Vec<String>, GhError> {
        Ok(Vec::new())
    }

    async fn test_summary(&self, _repo: &str, _head_sha: &str) -> Result<TestSummary, GhError> {
        Ok(TestSummary::default())
    }

    async fn list_workflow_runs(&self, _repo: &str) -> Result<Vec<WorkflowRun>, GhError> {
        Ok(Vec::new())
    }
}

// ---- fake persistence -----------------------------------------------------

/// An in-memory `PollStore`: a PR cache and etag map, with classifier config absent (defaults
/// apply) and CI history empty. `RefCell` gives the `&self` methods interior mutability without a
/// database.
#[derive(Default)]
struct FakeStore {
    cache: RefCell<HashMap<String, Vec<PullRequest>>>,
    etags: RefCell<HashMap<String, EtagRecord>>,
    saved_enrichments: RefCell<usize>,
}

impl PollStore for FakeStore {
    fn get_etag(&self, endpoint: &str) -> Result<Option<EtagRecord>, StoreError> {
        Ok(self.etags.borrow().get(endpoint).cloned())
    }
    fn set_etag(&self, endpoint: &str, record: &EtagRecord) -> Result<(), StoreError> {
        self.etags
            .borrow_mut()
            .insert(endpoint.to_string(), record.clone());
        Ok(())
    }
    fn load_repo_prs(&self, repo: &str) -> Result<Vec<PullRequest>, StoreError> {
        Ok(self.cache.borrow().get(repo).cloned().unwrap_or_default())
    }
    fn cache_repo_prs(&mut self, repo: &str, prs: &[PullRequest]) -> Result<(), StoreError> {
        self.cache
            .borrow_mut()
            .insert(repo.to_string(), prs.to_vec());
        Ok(())
    }
    fn save_enrichment(&mut self, _enrichment: &PrEnrichment) -> Result<(), StoreError> {
        *self.saved_enrichments.borrow_mut() += 1;
        Ok(())
    }
    fn load_label_map(&self, _repo: &str) -> Result<Option<LabelMap>, StoreError> {
        Ok(None)
    }
    fn load_bot_overrides(&self, _repo: &str) -> Result<Option<BotOverrides>, StoreError> {
        Ok(None)
    }
    fn get_correction(
        &self,
        _repo: &str,
        _number: u64,
    ) -> Result<Option<CategoryKind>, StoreError> {
        Ok(None)
    }
    fn recent_durations(
        &self,
        _repo: &str,
        _workflow: &str,
        _limit: usize,
    ) -> Result<Vec<u64>, StoreError> {
        Ok(Vec::new())
    }
    fn record_run(&self, _run: &WorkflowRun) -> Result<bool, StoreError> {
        Ok(true)
    }
}

// ---- tests ----------------------------------------------------------------

#[tokio::test]
async fn poll_once_against_fakes_adds_enriches_and_classifies() {
    let mut poller = Poller::new(
        FakeGh::modified(vec![sample_pr()]),
        FakeStore::default(),
        cadence(),
    );

    let outcome = poller.poll_once(&["o/r".to_string()]).await.unwrap();

    assert!(outcome.changed, "a fresh PR is a change");
    let added = outcome
        .events
        .iter()
        .filter(|e| matches!(e, ChangeEvent::Added(_)))
        .count();
    let enriched = outcome
        .events
        .iter()
        .filter(|e| matches!(e, ChangeEvent::Enriched(_)))
        .count();
    let classified = outcome
        .events
        .iter()
        .filter(|e| matches!(e, ChangeEvent::Classified(_)))
        .count();
    assert_eq!((added, enriched, classified), (1, 1, 1));
}

#[tokio::test]
async fn poll_once_propagates_gh_errors() {
    // The concrete-typed poller could only force this via a wiremock 401; the port makes it trivial.
    let mut poller = Poller::new(
        FakeGh::failing(GhError::Unauthorized),
        FakeStore::default(),
        cadence(),
    );

    let result = poller.poll_once(&["o/r".to_string()]).await;

    assert!(result.is_err(), "a failing GitHub call must sink the cycle");
}
