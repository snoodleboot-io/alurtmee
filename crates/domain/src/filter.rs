use std::collections::HashSet;

use crate::author_kind::AuthorKind;
use crate::category_kind::CategoryKind;

/// A composable, two-dimension toggle filter over the item stream (§3.6).
///
/// A `Filter` carries two independent dimensions:
/// - *source* — a set of [`AuthorKind`] values (human vs. bot), and
/// - *category* — a set of [`CategoryKind`] values (feature / security / unknown).
///
/// # Dimension algebra (§3.6)
///
/// Selections combine the way toggle-chips read to a user:
///
/// - **Within a dimension the selected values are OR'd.** Lighting up more chips in one row
///   *widens* that row: selecting both `Human` and `Bot` accepts items authored by *either*.
/// - **Across dimensions the results are AND'd.** Constraining two rows *narrows* overall: an item
///   must satisfy the source dimension **and** the category dimension.
/// - **An empty dimension is unconstrained.** No chips selected in a row means that row does not
///   filter at all — it accepts every value on that axis. An entirely empty `Filter` therefore
///   accepts everything.
///
/// Concretely, [`accepts`](Filter::accepts) evaluates:
///
/// ```text
/// (sources.is_empty()    || sources.contains(source))
///     && (categories.is_empty() || categories.contains(category))
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    sources: HashSet<AuthorKind>,
    categories: HashSet<CategoryKind>,
}

impl Filter {
    /// Create an empty filter. An empty filter constrains nothing and so accepts every item.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle a source chip: add it if absent, remove it if present.
    pub fn toggle_source(&mut self, source: AuthorKind) {
        if !self.sources.remove(&source) {
            self.sources.insert(source);
        }
    }

    /// Toggle a category chip: add it if absent, remove it if present.
    pub fn toggle_category(&mut self, category: CategoryKind) {
        if !self.categories.remove(&category) {
            self.categories.insert(category);
        }
    }

    /// Whether `source` is currently selected in the source dimension.
    pub fn is_source_active(&self, source: AuthorKind) -> bool {
        self.sources.contains(&source)
    }

    /// Whether `category` is currently selected in the category dimension.
    pub fn is_category_active(&self, category: CategoryKind) -> bool {
        self.categories.contains(&category)
    }

    /// Whether an item with the given `source` and `category` passes the filter.
    ///
    /// See the [type-level dimension algebra](Filter#dimension-algebra-36): OR within each
    /// dimension, AND across dimensions, with an empty dimension treated as unconstrained.
    pub fn accepts(&self, source: AuthorKind, category: CategoryKind) -> bool {
        (self.sources.is_empty() || self.sources.contains(&source))
            && (self.categories.is_empty() || self.categories.contains(&category))
    }

    /// Whether any dimension currently constrains the stream (any chip is selected).
    pub fn is_active(&self) -> bool {
        !self.sources.is_empty() || !self.categories.is_empty()
    }

    /// Total number of selected chips across both dimensions (for a "N filters" label).
    pub fn active_count(&self) -> usize {
        self.sources.len() + self.categories.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCES: [AuthorKind; 2] = [AuthorKind::Human, AuthorKind::Bot];
    const CATEGORIES: [CategoryKind; 3] = [
        CategoryKind::Feature,
        CategoryKind::Security,
        CategoryKind::Unknown,
    ];

    #[test]
    fn empty_filter_accepts_every_combination() {
        let f = Filter::new();
        for source in SOURCES {
            for category in CATEGORIES {
                assert!(
                    f.accepts(source, category),
                    "empty filter must accept ({source:?}, {category:?})"
                );
            }
        }
        assert!(!f.is_active());
        assert_eq!(f.active_count(), 0);
    }

    #[test]
    fn one_source_passes_only_that_source_for_any_category() {
        let mut f = Filter::new();
        f.toggle_source(AuthorKind::Human);
        for category in CATEGORIES {
            assert!(
                f.accepts(AuthorKind::Human, category),
                "selected source must pass for {category:?}"
            );
            assert!(
                !f.accepts(AuthorKind::Bot, category),
                "unselected source must be rejected for {category:?}"
            );
        }
    }

    #[test]
    fn one_category_passes_only_that_category_for_any_source() {
        let mut f = Filter::new();
        f.toggle_category(CategoryKind::Security);
        for source in SOURCES {
            assert!(
                f.accepts(source, CategoryKind::Security),
                "selected category must pass for {source:?}"
            );
            assert!(
                !f.accepts(source, CategoryKind::Feature),
                "unselected category must be rejected for {source:?}"
            );
            assert!(
                !f.accepts(source, CategoryKind::Unknown),
                "unselected category must be rejected for {source:?}"
            );
        }
    }

    #[test]
    fn both_dimensions_and_to_the_intersection() {
        let mut f = Filter::new();
        f.toggle_source(AuthorKind::Human);
        f.toggle_category(CategoryKind::Feature);

        // Only the exact intersection passes.
        assert!(f.accepts(AuthorKind::Human, CategoryKind::Feature));

        // Mismatched source, matched category -> reject.
        assert!(!f.accepts(AuthorKind::Bot, CategoryKind::Feature));
        // Matched source, mismatched category -> reject.
        assert!(!f.accepts(AuthorKind::Human, CategoryKind::Security));
        // Both mismatched -> reject.
        assert!(!f.accepts(AuthorKind::Bot, CategoryKind::Security));
    }

    #[test]
    fn or_within_source_dimension() {
        let mut f = Filter::new();
        f.toggle_source(AuthorKind::Human);
        f.toggle_source(AuthorKind::Bot);
        for category in CATEGORIES {
            assert!(f.accepts(AuthorKind::Human, category));
            assert!(f.accepts(AuthorKind::Bot, category));
        }
        assert_eq!(f.active_count(), 2);
    }

    #[test]
    fn or_within_category_dimension() {
        let mut f = Filter::new();
        f.toggle_category(CategoryKind::Feature);
        f.toggle_category(CategoryKind::Security);
        for source in SOURCES {
            assert!(f.accepts(source, CategoryKind::Feature));
            assert!(f.accepts(source, CategoryKind::Security));
            assert!(
                !f.accepts(source, CategoryKind::Unknown),
                "Unknown was not selected, must be rejected for {source:?}"
            );
        }
    }

    #[test]
    fn toggle_source_is_add_then_remove() {
        let mut f = Filter::new();
        assert!(!f.is_source_active(AuthorKind::Bot));

        f.toggle_source(AuthorKind::Bot);
        assert!(f.is_source_active(AuthorKind::Bot));
        assert_eq!(f.active_count(), 1);

        f.toggle_source(AuthorKind::Bot);
        assert!(!f.is_source_active(AuthorKind::Bot));
        assert_eq!(f.active_count(), 0);
    }

    #[test]
    fn toggle_category_is_add_then_remove() {
        let mut f = Filter::new();
        assert!(!f.is_category_active(CategoryKind::Unknown));

        f.toggle_category(CategoryKind::Unknown);
        assert!(f.is_category_active(CategoryKind::Unknown));
        assert_eq!(f.active_count(), 1);

        f.toggle_category(CategoryKind::Unknown);
        assert!(!f.is_category_active(CategoryKind::Unknown));
        assert_eq!(f.active_count(), 0);
    }

    #[test]
    fn is_active_tracks_any_selection() {
        let mut f = Filter::new();
        assert!(!f.is_active());

        f.toggle_source(AuthorKind::Human);
        assert!(f.is_active());

        f.toggle_category(CategoryKind::Feature);
        assert!(f.is_active());

        // Removing one dimension while the other remains constrained stays active.
        f.toggle_source(AuthorKind::Human);
        assert!(f.is_active());

        // Removing the last selection returns to inactive.
        f.toggle_category(CategoryKind::Feature);
        assert!(!f.is_active());
    }

    #[test]
    fn active_count_sums_both_dimensions() {
        let mut f = Filter::new();
        f.toggle_source(AuthorKind::Human);
        f.toggle_source(AuthorKind::Bot);
        f.toggle_category(CategoryKind::Feature);
        assert_eq!(f.active_count(), 3);
    }

    #[test]
    fn new_equals_default() {
        assert_eq!(Filter::new(), Filter::default());
    }
}
