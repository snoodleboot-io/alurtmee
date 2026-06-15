/// A "this run was too slow" verdict, carrying the human-readable reason it fired.
///
/// The reason names the actual numbers (duration vs baseline) so the flag is explainable, not a
/// bare boolean — the same auditability principle as the classification signals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlowFlag {
    /// Human-readable explanation, e.g. "ran 540s — over the p90 baseline of 300s (last 12 runs)".
    pub reason: String,
}
