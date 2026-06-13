use serde::{Deserialize, Serialize};

use crate::author::Author;
use crate::category::Category;

/// A single tracked unit of work surfaced in the dashboard (currently an open pull request).
///
/// Phase 0 carries the minimal identifying shape; enrichment fields (reviews, checks, CI timing)
/// attach in later phases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    pub title: String,
    pub author: Author,
    pub category: Category,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::author_kind::AuthorKind;
    use crate::category_kind::CategoryKind;

    #[test]
    fn item_serde_round_trip_preserves_equality() {
        let item = Item {
            id: 42,
            title: "Patch CVE-2026-0001".to_string(),
            author: Author {
                login: "dependabot[bot]".to_string(),
                kind: AuthorKind::Bot,
            },
            category: Category {
                kind: CategoryKind::Security,
                confidence: 0.9,
                signal: "advisory".to_string(),
            },
        };

        let json = serde_json::to_string(&item).expect("serialize Item");
        let decoded: Item = serde_json::from_str(&json).expect("deserialize Item");

        assert_eq!(item, decoded);
    }
}
