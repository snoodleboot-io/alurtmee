use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// The set of repositories the user has chosen to poll, identified by `owner/name` slug.
///
/// A `BTreeSet` keeps the selection deduplicated and in a deterministic order, so the persisted
/// JSON is stable across saves (no spurious diffs) and restart restores an identical set. This is
/// the unit `store` round-trips through the `config` table to satisfy the "selection survives
/// restart" exit criterion.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSelection {
    full_names: BTreeSet<String>,
}

impl RepoSelection {
    /// An empty selection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `full_name` (an `owner/name` slug) is currently selected.
    pub fn contains(&self, full_name: &str) -> bool {
        self.full_names.contains(full_name)
    }

    /// Add a repository to the selection. Returns `true` if it was newly inserted.
    pub fn insert(&mut self, full_name: impl Into<String>) -> bool {
        self.full_names.insert(full_name.into())
    }

    /// Remove a repository from the selection. Returns `true` if it was present.
    pub fn remove(&mut self, full_name: &str) -> bool {
        self.full_names.remove(full_name)
    }

    /// Toggle membership: select if absent, deselect if present. Returns the new membership state.
    pub fn toggle(&mut self, full_name: impl Into<String>) -> bool {
        let full_name = full_name.into();
        if self.full_names.remove(&full_name) {
            false
        } else {
            self.full_names.insert(full_name);
            true
        }
    }

    /// Number of selected repositories.
    pub fn len(&self) -> usize {
        self.full_names.len()
    }

    /// Whether the selection is empty.
    pub fn is_empty(&self) -> bool {
        self.full_names.is_empty()
    }

    /// Iterate over selected `owner/name` slugs in sorted order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.full_names.iter().map(String::as_str)
    }
}

impl FromIterator<String> for RepoSelection {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self {
            full_names: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_selection_is_empty() {
        let selection = RepoSelection::new();
        assert!(selection.is_empty());
        assert_eq!(selection.len(), 0);
    }

    #[test]
    fn insert_is_idempotent_and_reports_novelty() {
        let mut selection = RepoSelection::new();
        assert!(selection.insert("octocat/hello"));
        assert!(!selection.insert("octocat/hello"));
        assert_eq!(selection.len(), 1);
        assert!(selection.contains("octocat/hello"));
    }

    #[test]
    fn toggle_flips_membership() {
        let mut selection = RepoSelection::new();
        assert!(selection.toggle("a/b"), "absent → selected");
        assert!(selection.contains("a/b"));
        assert!(!selection.toggle("a/b"), "present → deselected");
        assert!(!selection.contains("a/b"));
    }

    #[test]
    fn remove_reports_prior_presence() {
        let mut selection = RepoSelection::new();
        selection.insert("a/b");
        assert!(selection.remove("a/b"));
        assert!(!selection.remove("a/b"));
    }

    #[test]
    fn iter_yields_sorted_unique_slugs() {
        let selection: RepoSelection = ["c/c", "a/a", "b/b", "a/a"]
            .into_iter()
            .map(String::from)
            .collect();
        let slugs: Vec<&str> = selection.iter().collect();
        assert_eq!(slugs, vec!["a/a", "b/b", "c/c"]);
    }

    #[test]
    fn json_round_trip_preserves_selection() {
        let selection: RepoSelection = ["octocat/hello", "rust-lang/rust"]
            .into_iter()
            .map(String::from)
            .collect();
        let json = serde_json::to_string(&selection).expect("serialize");
        let restored: RepoSelection = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(selection, restored);
    }
}
