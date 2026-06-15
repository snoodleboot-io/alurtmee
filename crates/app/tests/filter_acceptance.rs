//! Phase 6 acceptance (ATDD) — composable filters over a fixture feed.
//!
//! Replicates exactly what the UI's `visible_prs` does — for each PR, look up its classification and
//! keep it iff the filter accepts `(source, category)`; unclassified PRs are always shown — and
//! asserts that toggling chips narrows the feed correctly across combined dimensions. No PAT / no
//! network: the filter engine is pure (`domain::Filter`), so this is fully verifiable now.

use domain::{AuthorKind, Category, CategoryKind, Classification, Filter, PrId, PullRequest};

fn pr(number: u64) -> PullRequest {
    PullRequest {
        id: PrId::new("o/r", number),
        title: format!("PR {number}"),
        author: "octocat".to_string(),
        draft: false,
        updated_at: "t1".to_string(),
        url: String::new(),
        head_sha: String::new(),
        author_type: String::new(),
        head_ref: String::new(),
        labels: Vec::new(),
    }
}

fn classification(number: u64, source: AuthorKind, category: CategoryKind) -> Classification {
    Classification {
        id: PrId::new("o/r", number),
        author_kind: source,
        category: Category {
            kind: category,
            confidence: 0.9,
            signal: "test".to_string(),
        },
    }
}

/// A 5-item feed spanning the dimensions, plus one unclassified PR (#5).
fn feed() -> Vec<(PullRequest, Option<Classification>)> {
    use AuthorKind::{Bot, Human};
    use CategoryKind::{Feature, Security};
    vec![
        (pr(1), Some(classification(1, Human, Feature))),
        (pr(2), Some(classification(2, Bot, Security))),
        (pr(3), Some(classification(3, Human, Security))),
        (pr(4), Some(classification(4, Bot, Feature))),
        (pr(5), None), // unclassified
    ]
}

/// The UI's filter rule applied to the feed, returning the visible PR numbers.
fn visible(feed: &[(PullRequest, Option<Classification>)], filter: &Filter) -> Vec<u64> {
    feed.iter()
        .filter(|(_, c)| match c {
            Some(c) => filter.accepts(c.author_kind, c.category.kind),
            None => true,
        })
        .map(|(pr, _)| pr.id.number)
        .collect()
}

#[test]
fn empty_filter_shows_the_whole_feed() {
    let feed = feed();
    assert_eq!(visible(&feed, &Filter::new()), vec![1, 2, 3, 4, 5]);
}

#[test]
fn one_source_chip_narrows_by_source_and_keeps_unclassified() {
    let feed = feed();
    let mut filter = Filter::new();
    filter.toggle_source(AuthorKind::Human);
    // Human PRs (#1, #3) + the always-shown unclassified (#5).
    assert_eq!(visible(&feed, &filter), vec![1, 3, 5]);
}

#[test]
fn source_and_category_chips_and_to_the_intersection() {
    let feed = feed();
    let mut filter = Filter::new();
    filter.toggle_source(AuthorKind::Bot);
    filter.toggle_category(CategoryKind::Security);
    // Bot AND Security → #2 only, plus unclassified #5.
    assert_eq!(visible(&feed, &filter), vec![2, 5]);
}

#[test]
fn or_within_category_dimension_widens() {
    let feed = feed();
    let mut filter = Filter::new();
    filter.toggle_category(CategoryKind::Feature);
    filter.toggle_category(CategoryKind::Security);
    // Feature OR Security covers all classified PRs (#1-#4) + unclassified #5.
    assert_eq!(visible(&feed, &filter), vec![1, 2, 3, 4, 5]);
}

#[test]
fn toggling_a_chip_off_restores_the_feed() {
    let feed = feed();
    let mut filter = Filter::new();
    filter.toggle_source(AuthorKind::Human);
    assert_eq!(visible(&feed, &filter), vec![1, 3, 5]);
    filter.toggle_source(AuthorKind::Human); // off again
    assert_eq!(visible(&feed, &filter), vec![1, 2, 3, 4, 5]);
    assert!(!filter.is_active());
}
