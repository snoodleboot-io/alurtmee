//! Rolling-baseline slow-CI detection (AD-5, pure).
//!
//! **Why a rolling percentile, not a fixed threshold, as the default (§3.6):** every workflow has
//! its own normal duration — a 20-second lint and a 40-minute integration suite shouldn't share one
//! threshold, and hand-tuning a threshold per workflow per repo doesn't scale. A rolling p75/p90
//! over the recent runs *learns* each workflow's normal and flags genuine regressions; the
//! percentile (vs the mean) is robust to the occasional freak run. The fixed `threshold_secs` is
//! only a cold-start fallback while a workflow hasn't yet accumulated enough samples — flagged as
//! such in the reason so the user knows the baseline is still warming up.

use crate::slow_ci_config::SlowCiConfig;
use crate::slow_flag::SlowFlag;

/// Decide whether `current_secs` is too slow for a workflow, given its recent completed-run
/// durations (`history`, any order) and the policy.
///
/// With at least `min_samples` history points, the run is slow iff it exceeds the configured
/// percentile of the history. Otherwise the baseline is still warming up and we fall back to the
/// fixed threshold. Returns `Some(SlowFlag)` with a numbers-named reason when slow, else `None`.
pub fn flag_slow(history: &[u64], current_secs: u64, config: &SlowCiConfig) -> Option<SlowFlag> {
    if history.len() >= config.min_samples {
        let baseline = config.percentile.of(history)?;
        if current_secs > baseline {
            return Some(SlowFlag {
                reason: format!(
                    "ran {current_secs}s — over the p{} baseline of {baseline}s (last {} runs)",
                    config.percentile.label(),
                    history.len(),
                ),
            });
        }
        return None;
    }

    // Cold start: not enough samples for a trustworthy percentile.
    if current_secs > config.threshold_secs {
        return Some(SlowFlag {
            reason: format!(
                "ran {current_secs}s — over the {}s threshold (baseline warming up, {}/{} runs)",
                config.threshold_secs,
                history.len(),
                config.min_samples,
            ),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percentile::Percentile;

    fn config() -> SlowCiConfig {
        SlowCiConfig {
            threshold_secs: 600,
            min_samples: 5,
            percentile: Percentile::P90,
        }
    }

    #[test]
    fn cold_start_under_threshold_is_not_slow() {
        assert!(flag_slow(&[100, 120], 300, &config()).is_none());
    }

    #[test]
    fn cold_start_over_threshold_flags_with_warming_up_reason() {
        let flag = flag_slow(&[100], 900, &config()).expect("flagged");
        assert!(flag.reason.contains("threshold"));
        assert!(flag.reason.contains("warming up"));
        assert!(flag.reason.contains("900s"));
    }

    #[test]
    fn with_baseline_under_percentile_is_not_slow() {
        // Ten ~100s runs; a 110s run is under p90 (=100? nearest-rank of constant is 100) -> not slow.
        let history = vec![100u64; 10];
        assert!(flag_slow(&history, 100, &config()).is_none());
    }

    #[test]
    fn with_baseline_over_percentile_flags_with_percentile_reason() {
        // history p90 = 200 (nearest-rank index 8 of sorted) ; current 500 > 200 -> slow.
        let history = vec![100, 110, 120, 130, 140, 150, 160, 180, 200, 210];
        // sorted same; ceil(0.9*10)=9 -> index 8 -> 200
        let flag = flag_slow(&history, 500, &config()).expect("flagged");
        assert!(
            flag.reason.contains("p90 baseline of 200s"),
            "{}",
            flag.reason
        );
        assert!(flag.reason.contains("last 10 runs"));
        assert!(flag.reason.contains("500s"));
    }

    #[test]
    fn baseline_is_robust_to_outliers() {
        // Nine 100s runs + one 10000s freak: p90 stays at 100, so a 200s run is still flagged
        // (a mean-based baseline would be ~1090s and would miss it).
        let mut history = vec![100u64; 9];
        history.push(10_000);
        assert!(flag_slow(&history, 200, &config()).is_some());
    }

    #[test]
    fn exactly_at_baseline_is_not_slow() {
        let history = vec![100, 110, 120, 130, 140, 150, 160, 180, 200, 210];
        // current == p90 (200) is not "over" → not slow.
        assert!(flag_slow(&history, 200, &config()).is_none());
    }
}
