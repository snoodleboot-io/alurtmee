use serde::{Deserialize, Serialize};

use crate::category_kind::CategoryKind;

/// The result of feature-vs-security classification for an item.
///
/// `signal` records *which* layer fired (label, prefix, path, advisory) so a heuristic decision
/// stays auditable and user-correctable (AD-5). `confidence` is in `[0.0, 1.0]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub kind: CategoryKind,
    pub confidence: f32,
    pub signal: String,
}
