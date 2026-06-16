//! The persistence port the poller depends on (DIP, ARD AD-3).
//!
//! `Store` is a broad multi-purpose object; the poller touches only the etag cache, the PR
//! snapshot cache, the classifier config, and the CI-run history. This poller-owned trait names
//! exactly that slice, so the poll loop depends on an interface it defines rather than the concrete
//! store — making it unit-testable against an in-memory fake and keeping the seam honest about what
//! the poller actually persists. The concrete [`Store`] satisfies it by delegating to its inherent
//! methods, so behaviour is unchanged.

use domain::{BotOverrides, CategoryKind, LabelMap, PrEnrichment, PullRequest, WorkflowRun};
use store::{EtagRecord, Store, StoreError};

/// The slice of persistence the [`Poller`](crate::Poller) actually uses.
pub trait PollStore {
    /// Read the persisted ETag record for an endpoint key, if any.
    fn get_etag(&self, endpoint: &str) -> Result<Option<EtagRecord>, StoreError>;

    /// Persist the ETag record for an endpoint key.
    fn set_etag(&self, endpoint: &str, record: &EtagRecord) -> Result<(), StoreError>;

    /// Load the cached open-PR snapshot for a repo.
    fn load_repo_prs(&self, repo: &str) -> Result<Vec<PullRequest>, StoreError>;

    /// Replace the cached open-PR snapshot for a repo.
    fn cache_repo_prs(&mut self, repo: &str, prs: &[PullRequest]) -> Result<(), StoreError>;

    /// Persist a PR's enrichment (reviews, comments, reconciled test verdict).
    fn save_enrichment(&mut self, enrichment: &PrEnrichment) -> Result<(), StoreError>;

    /// Load the per-repo label→category map override, if the user configured one.
    fn load_label_map(&self, repo: &str) -> Result<Option<LabelMap>, StoreError>;

    /// Load the per-repo bot-classification overrides, if any.
    fn load_bot_overrides(&self, repo: &str) -> Result<Option<BotOverrides>, StoreError>;

    /// Load a user's category correction for a specific PR, if one was recorded.
    fn get_correction(&self, repo: &str, number: u64) -> Result<Option<CategoryKind>, StoreError>;

    /// The most recent completed-run durations for a `(repo, workflow)`, newest first.
    fn recent_durations(
        &self,
        repo: &str,
        workflow: &str,
        limit: usize,
    ) -> Result<Vec<u64>, StoreError>;

    /// Record a CI run, returning `true` if it was newly seen (so callers de-dupe alerts).
    fn record_run(&self, run: &WorkflowRun) -> Result<bool, StoreError>;
}

/// The production adapter: delegate straight through to the concrete store's inherent methods.
impl PollStore for Store {
    fn get_etag(&self, endpoint: &str) -> Result<Option<EtagRecord>, StoreError> {
        Store::get_etag(self, endpoint)
    }

    fn set_etag(&self, endpoint: &str, record: &EtagRecord) -> Result<(), StoreError> {
        Store::set_etag(self, endpoint, record)
    }

    fn load_repo_prs(&self, repo: &str) -> Result<Vec<PullRequest>, StoreError> {
        Store::load_repo_prs(self, repo)
    }

    fn cache_repo_prs(&mut self, repo: &str, prs: &[PullRequest]) -> Result<(), StoreError> {
        Store::cache_repo_prs(self, repo, prs)
    }

    fn save_enrichment(&mut self, enrichment: &PrEnrichment) -> Result<(), StoreError> {
        Store::save_enrichment(self, enrichment)
    }

    fn load_label_map(&self, repo: &str) -> Result<Option<LabelMap>, StoreError> {
        Store::load_label_map(self, repo)
    }

    fn load_bot_overrides(&self, repo: &str) -> Result<Option<BotOverrides>, StoreError> {
        Store::load_bot_overrides(self, repo)
    }

    fn get_correction(&self, repo: &str, number: u64) -> Result<Option<CategoryKind>, StoreError> {
        Store::get_correction(self, repo, number)
    }

    fn recent_durations(
        &self,
        repo: &str,
        workflow: &str,
        limit: usize,
    ) -> Result<Vec<u64>, StoreError> {
        Store::recent_durations(self, repo, workflow, limit)
    }

    fn record_run(&self, run: &WorkflowRun) -> Result<bool, StoreError> {
        Store::record_run(self, run)
    }
}
