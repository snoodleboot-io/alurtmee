use std::collections::BTreeMap;

use domain::{ChangeEvent, Classification, PrEnrichment, PrId, PullRequest};

/// The UI's view of currently-open pull requests plus their enrichment and classification,
/// maintained incrementally from [`ChangeEvent`]s.
///
/// Pure and synchronous so it can be unit-tested without the poller or Iced. The list is kept
/// sorted by [`domain::PrId`] (`repo`, then `number`) so the rendered order is stable across
/// updates and doesn't jump around as events arrive. Enrichment (reviews/comments/tests) and
/// classification (human/bot + feature/security) are held in side maps keyed by id.
#[derive(Debug, Clone, Default)]
pub struct PrListModel {
    prs: Vec<PullRequest>,
    enrichments: BTreeMap<PrId, PrEnrichment>,
    classifications: BTreeMap<PrId, Classification>,
}

impl PrListModel {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current open pull requests, sorted by identity.
    pub fn prs(&self) -> &[PullRequest] {
        &self.prs
    }

    /// The enrichment for a PR, if it has been fetched yet.
    pub fn enrichment(&self, id: &PrId) -> Option<&PrEnrichment> {
        self.enrichments.get(id)
    }

    /// The classification verdict for a PR, if it has been computed yet.
    pub fn classification(&self, id: &PrId) -> Option<&Classification> {
        self.classifications.get(id)
    }

    /// Number of open pull requests currently shown.
    pub fn len(&self) -> usize {
        self.prs.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.prs.is_empty()
    }

    /// Override the locally-displayed category for a PR (immediate feedback after a user
    /// correction; the durable correction is persisted separately and re-applied on the next poll).
    pub fn set_corrected_category(&mut self, id: &PrId, category: domain::Category) {
        if let Some(classification) = self.classifications.get_mut(id) {
            classification.category = category;
        }
    }

    /// Apply one change event. `Added`/`Updated` upsert the PR by id; `Removed` drops the PR and
    /// its derived data; `Enriched`/`Classified` record the enrichment/classification for their PR.
    /// The list stays sorted by id.
    pub fn apply(&mut self, event: ChangeEvent) {
        match event {
            ChangeEvent::Added(pr) | ChangeEvent::Updated(pr) => {
                match self.prs.iter_mut().find(|existing| existing.id == pr.id) {
                    Some(existing) => *existing = pr,
                    None => self.prs.push(pr),
                }
            }
            ChangeEvent::Removed(id) => {
                self.prs.retain(|pr| pr.id != id);
                self.enrichments.remove(&id);
                self.classifications.remove(&id);
            }
            ChangeEvent::Enriched(enrichment) => {
                self.enrichments.insert(enrichment.id.clone(), enrichment);
            }
            ChangeEvent::Classified(classification) => {
                self.classifications
                    .insert(classification.id.clone(), classification);
            }
        }
        self.prs.sort_by(|a, b| a.id.cmp(&b.id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::PrId;

    fn pr(repo: &str, number: u64, updated_at: &str) -> PullRequest {
        PullRequest {
            id: PrId::new(repo, number),
            title: format!("PR {number}"),
            author: "octocat".to_string(),
            draft: false,
            updated_at: updated_at.to_string(),
            url: format!("https://github.com/{repo}/pull/{number}"),
            head_sha: String::new(),
            author_type: String::new(),
            head_ref: String::new(),
            labels: Vec::new(),
        }
    }

    #[test]
    fn added_events_populate_sorted() {
        let mut model = PrListModel::new();
        model.apply(ChangeEvent::Added(pr("o/r", 2, "t1")));
        model.apply(ChangeEvent::Added(pr("o/r", 1, "t1")));
        let numbers: Vec<u64> = model.prs().iter().map(|p| p.id.number).collect();
        assert_eq!(numbers, vec![1, 2], "kept sorted by id");
    }

    #[test]
    fn updated_replaces_in_place() {
        let mut model = PrListModel::new();
        model.apply(ChangeEvent::Added(pr("o/r", 1, "t1")));
        model.apply(ChangeEvent::Updated(pr("o/r", 1, "t2")));
        assert_eq!(model.len(), 1);
        assert_eq!(model.prs()[0].updated_at, "t2");
    }

    #[test]
    fn added_is_idempotent_upsert() {
        let mut model = PrListModel::new();
        model.apply(ChangeEvent::Added(pr("o/r", 1, "t1")));
        model.apply(ChangeEvent::Added(pr("o/r", 1, "t1")));
        assert_eq!(model.len(), 1, "duplicate Added does not double-insert");
    }

    #[test]
    fn removed_drops_the_pr() {
        let mut model = PrListModel::new();
        model.apply(ChangeEvent::Added(pr("o/r", 1, "t1")));
        model.apply(ChangeEvent::Added(pr("o/r", 2, "t1")));
        model.apply(ChangeEvent::Removed(PrId::new("o/r", 1)));
        let numbers: Vec<u64> = model.prs().iter().map(|p| p.id.number).collect();
        assert_eq!(numbers, vec![2]);
    }

    #[test]
    fn removed_unknown_is_a_noop() {
        let mut model = PrListModel::new();
        model.apply(ChangeEvent::Added(pr("o/r", 1, "t1")));
        model.apply(ChangeEvent::Removed(PrId::new("o/r", 99)));
        assert_eq!(model.len(), 1);
    }

    #[test]
    fn classified_event_is_recorded_and_dropped_on_remove() {
        use domain::{AuthorKind, Category, CategoryKind, Classification};
        let id = PrId::new("o/r", 1);
        let classification = Classification {
            id: id.clone(),
            author_kind: AuthorKind::Bot,
            category: Category {
                kind: CategoryKind::Security,
                confidence: 0.9,
                signal: "dependabot".to_string(),
            },
        };
        let mut model = PrListModel::new();
        model.apply(ChangeEvent::Added(pr("o/r", 1, "t1")));
        model.apply(ChangeEvent::Classified(classification));
        assert_eq!(
            model.classification(&id).unwrap().author_kind,
            AuthorKind::Bot
        );

        model.apply(ChangeEvent::Removed(id.clone()));
        assert!(model.classification(&id).is_none());
    }

    #[test]
    fn set_corrected_category_overrides_locally() {
        use domain::{AuthorKind, Category, CategoryKind, Classification};
        let id = PrId::new("o/r", 1);
        let mut model = PrListModel::new();
        model.apply(ChangeEvent::Classified(Classification {
            id: id.clone(),
            author_kind: AuthorKind::Human,
            category: Category {
                kind: CategoryKind::Unknown,
                confidence: 0.0,
                signal: "none".to_string(),
            },
        }));
        model.set_corrected_category(
            &id,
            Category {
                kind: CategoryKind::Feature,
                confidence: 1.0,
                signal: "correction".to_string(),
            },
        );
        assert_eq!(
            model.classification(&id).unwrap().category.kind,
            CategoryKind::Feature
        );
    }
}
