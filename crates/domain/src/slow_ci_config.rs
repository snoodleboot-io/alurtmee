use serde::{Deserialize, Serialize};

use crate::percentile::Percentile;

/// Policy for flagging a workflow run as too slow.
///
/// Default behaviour is the **rolling percentile** (adapts per workflow, no tuning); until a
/// workflow has accumulated `min_samples` completed runs the baseline is still warming up, so we
/// fall back to a **fixed `threshold_secs`** to avoid both silence and noise on a cold start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlowCiConfig {
    /// Fixed fallback threshold (seconds) used until enough samples exist.
    pub threshold_secs: u64,
    /// Completed-run samples required before the percentile baseline is trusted.
    pub min_samples: usize,
    /// Which percentile to use as the baseline.
    pub percentile: Percentile,
}

impl Default for SlowCiConfig {
    fn default() -> Self {
        Self {
            threshold_secs: 600, // 10 minutes
            min_samples: 5,
            percentile: Percentile::P90,
        }
    }
}
