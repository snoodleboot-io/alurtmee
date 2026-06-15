use serde::{Deserialize, Serialize};

/// The overall CI verdict for a pull request, shown as a single badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TestState {
    /// No checks or status reported.
    #[default]
    None,
    /// At least one check still running/queued and nothing failing.
    Pending,
    /// All completed checks succeeded (and at least one ran).
    Passing,
    /// At least one check concluded in failure.
    Failing,
}
