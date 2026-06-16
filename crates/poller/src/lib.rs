//! `poller` — polling scheduler and change-detection for Alurtmee.
//!
//! Drives the cheap change-detection tier (conditional `GET .../pulls?state=open`, ARD AD-3):
//! lists open PRs for the selected repos, diffs them against the cached state, and emits
//! [`domain::ChangeEvent`]s the UI subscribes to. The scheduler uses an adaptive cadence with
//! jitter so idle polling is cheap and well-behaved (free 304s, no thundering herd).

mod diff;
mod gh_api;
mod poll_error;
mod poll_outcome;
mod poll_store;
mod poller;

pub use diff::diff_pull_requests;
pub use domain::ChangeEvent;
pub use gh_api::GhApi;
pub use poll_error::PollError;
pub use poll_outcome::PollOutcome;
pub use poll_store::PollStore;
pub use poller::Poller;
