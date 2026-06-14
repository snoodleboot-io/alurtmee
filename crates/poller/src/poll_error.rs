use gh_client::GhError;
use store::StoreError;

/// Errors a poll cycle can surface: a GitHub request failure or a persistence failure.
///
/// The scheduler logs and backs off on these rather than terminating, so a transient network blip
/// doesn't kill the poller (see [`crate::Poller::run`]).
#[derive(Debug, thiserror::Error)]
pub enum PollError {
    #[error("github request failed: {0}")]
    GitHub(#[from] GhError),
    #[error("store access failed: {0}")]
    Store(#[from] StoreError),
}
