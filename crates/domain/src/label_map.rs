use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::category_kind::CategoryKind;

/// Configurable mapping from a PR label name to a category.
///
/// Labels are the **highest-precedence** classification signal (AD-5): an explicit label is a
/// human's deliberate statement of intent, so it should outrank any heuristic guess. The map is
/// per-repo and persisted, so teams can teach the classifier their own label conventions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelMap {
    map: BTreeMap<String, CategoryKind>,
}

impl LabelMap {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// A starter map covering common label conventions, as a sensible default for a new repo.
    pub fn with_common_defaults() -> Self {
        let mut map = Self::new();
        for label in ["security", "vulnerability", "cve"] {
            map.insert(label, CategoryKind::Security);
        }
        for label in ["feature", "enhancement", "feat"] {
            map.insert(label, CategoryKind::Feature);
        }
        map
    }

    /// Map `label` (case-insensitive) to `kind`.
    pub fn insert(&mut self, label: impl Into<String>, kind: CategoryKind) -> &mut Self {
        self.map.insert(label.into().to_ascii_lowercase(), kind);
        self
    }

    /// The category mapped to `label`, if any (case-insensitive lookup).
    pub fn get(&self, label: &str) -> Option<CategoryKind> {
        self.map.get(&label.to_ascii_lowercase()).copied()
    }

    /// Whether the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Number of label rules.
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_map_is_empty_and_len_tracks_inserts() {
        let mut map = LabelMap::new();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
        map.insert("security", CategoryKind::Security);
        map.insert("feature", CategoryKind::Feature);
        assert!(!map.is_empty());
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let mut map = LabelMap::new();
        map.insert("Security", CategoryKind::Security);
        assert_eq!(map.get("security"), Some(CategoryKind::Security));
        assert_eq!(map.get("SECURITY"), Some(CategoryKind::Security));
        assert_eq!(map.get("feature"), None);
    }

    #[test]
    fn common_defaults_cover_both_categories() {
        let map = LabelMap::with_common_defaults();
        assert_eq!(map.get("vulnerability"), Some(CategoryKind::Security));
        assert_eq!(map.get("enhancement"), Some(CategoryKind::Feature));
    }

    #[test]
    fn json_round_trip() {
        let map = LabelMap::with_common_defaults();
        let json = serde_json::to_string(&map).unwrap();
        let back: LabelMap = serde_json::from_str(&json).unwrap();
        assert_eq!(map, back);
    }
}
