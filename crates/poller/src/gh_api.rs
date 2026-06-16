//! The GitHub-access port the poller depends on (DIP, ARD AD-3).
//!
//! The poller needs only six of `GhClient`'s methods. Depending on this narrow, poller-owned trait
//! — rather than the concrete client — inverts the dependency: the client implements an interface
//! the *consumer* defines, so the poller can be unit-tested against a fake that records calls and
//! forces error paths, with no `wiremock` server or real token. The concrete [`GhClient`] satisfies
//! it by delegating to its inherent methods (see the impl below), so behaviour is unchanged.
//!
//! Each method returns `impl Future<…> + Send` so the poll loop can be spawned on a background
//! Tokio task without boxing or an `async-trait` dependency.

use std::future::Future;

use domain::{Comment, Review, TestSummary, WorkflowRun};
use gh_client::{GhClient, GhError, OpenPrsResult};

/// The slice of GitHub access the [`Poller`](crate::Poller) actually uses.
pub trait GhApi {
    /// Conditionally list a repo's open PRs (keyed on `etag`); see [`GhClient::list_open_prs`].
    fn list_open_prs(
        &self,
        repo: &str,
        etag: Option<&str>,
    ) -> impl Future<Output = Result<OpenPrsResult, GhError>> + Send;

    /// Fetch the reviews on a PR; see [`GhClient::list_reviews`].
    fn list_reviews(
        &self,
        repo: &str,
        number: u64,
    ) -> impl Future<Output = Result<Vec<Review>, GhError>> + Send;

    /// Fetch the issue + review comments on a PR; see [`GhClient::list_comments`].
    fn list_comments(
        &self,
        repo: &str,
        number: u64,
    ) -> impl Future<Output = Result<Vec<Comment>, GhError>> + Send;

    /// Fetch the changed file paths of a PR; see [`GhClient::list_changed_paths`].
    fn list_changed_paths(
        &self,
        repo: &str,
        number: u64,
    ) -> impl Future<Output = Result<Vec<String>, GhError>> + Send;

    /// Reconcile a head SHA's check-runs + status into a test summary; see
    /// [`GhClient::test_summary`].
    fn test_summary(
        &self,
        repo: &str,
        head_sha: &str,
    ) -> impl Future<Output = Result<TestSummary, GhError>> + Send;

    /// Fetch the repo's recent Actions runs; see [`GhClient::list_workflow_runs`].
    fn list_workflow_runs(
        &self,
        repo: &str,
    ) -> impl Future<Output = Result<Vec<WorkflowRun>, GhError>> + Send;
}

/// The production adapter: delegate straight through to the concrete client's inherent methods.
impl GhApi for GhClient {
    fn list_open_prs(
        &self,
        repo: &str,
        etag: Option<&str>,
    ) -> impl Future<Output = Result<OpenPrsResult, GhError>> + Send {
        GhClient::list_open_prs(self, repo, etag)
    }

    fn list_reviews(
        &self,
        repo: &str,
        number: u64,
    ) -> impl Future<Output = Result<Vec<Review>, GhError>> + Send {
        GhClient::list_reviews(self, repo, number)
    }

    fn list_comments(
        &self,
        repo: &str,
        number: u64,
    ) -> impl Future<Output = Result<Vec<Comment>, GhError>> + Send {
        GhClient::list_comments(self, repo, number)
    }

    fn list_changed_paths(
        &self,
        repo: &str,
        number: u64,
    ) -> impl Future<Output = Result<Vec<String>, GhError>> + Send {
        GhClient::list_changed_paths(self, repo, number)
    }

    fn test_summary(
        &self,
        repo: &str,
        head_sha: &str,
    ) -> impl Future<Output = Result<TestSummary, GhError>> + Send {
        GhClient::test_summary(self, repo, head_sha)
    }

    fn list_workflow_runs(
        &self,
        repo: &str,
    ) -> impl Future<Output = Result<Vec<WorkflowRun>, GhError>> + Send {
        GhClient::list_workflow_runs(self, repo)
    }
}
