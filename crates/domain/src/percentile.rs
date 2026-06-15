use serde::{Deserialize, Serialize};

/// Which rolling percentile to use as the "too slow" baseline.
///
/// p75/p90 (not the mean) are used deliberately: a percentile is robust to outliers — one freak
/// 30-minute run won't drag the baseline up the way an average would — and it adapts per workflow
/// without any manual per-repo tuning (AD-5, §3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Percentile {
    P75,
    P90,
}

impl Percentile {
    /// The fraction in `(0, 1]`.
    pub fn fraction(self) -> f64 {
        match self {
            Percentile::P75 => 0.75,
            Percentile::P90 => 0.90,
        }
    }

    /// Short label for reason strings (`75` / `90`).
    pub fn label(self) -> &'static str {
        match self {
            Percentile::P75 => "75",
            Percentile::P90 => "90",
        }
    }

    /// The nearest-rank percentile of `values` (any order), or `None` if empty.
    ///
    /// Nearest-rank (`ceil(p·n)`) is deterministic and needs no interpolation — a good fit for the
    /// small windows (tens of runs) we keep per workflow.
    pub fn of(self, values: &[u64]) -> Option<u64> {
        if values.is_empty() {
            return None;
        }
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        let n = sorted.len();
        let rank = (self.fraction() * n as f64).ceil() as usize;
        let index = rank.clamp(1, n) - 1;
        Some(sorted[index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_has_no_percentile() {
        assert_eq!(Percentile::P90.of(&[]), None);
    }

    #[test]
    fn nearest_rank_p90_of_ten() {
        let v: Vec<u64> = (1..=10).collect(); // 1..10
                                              // ceil(0.9*10)=9 → index 8 → value 9
        assert_eq!(Percentile::P90.of(&v), Some(9));
        // ceil(0.75*10)=8 → index 7 → value 8
        assert_eq!(Percentile::P75.of(&v), Some(8));
    }

    #[test]
    fn order_independent() {
        assert_eq!(
            Percentile::P75.of(&[30, 10, 20, 40]),
            Percentile::P75.of(&[40, 30, 20, 10])
        );
    }

    #[test]
    fn robust_to_a_single_outlier() {
        // Nine ~100s runs and one 10000s freak. p90 stays near the cluster, unlike a mean.
        let mut v = vec![100u64; 9];
        v.push(10_000);
        assert_eq!(Percentile::P90.of(&v), Some(100));
    }
}
