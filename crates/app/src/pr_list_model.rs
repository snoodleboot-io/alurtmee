use domain::{ChangeEvent, PullRequest};

/// The UI's view of currently-open pull requests, maintained incrementally from [`ChangeEvent`]s.
///
/// Pure and synchronous so it can be unit-tested without the poller or Iced. The list is kept
/// sorted by [`domain::PrId`] (`repo`, then `number`) so the rendered order is stable across
/// updates and doesn't jump around as events arrive.
#[derive(Debug, Clone, Default)]
pub struct PrListModel {
    prs: Vec<PullRequest>,
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

    /// Number of open pull requests currently shown.
    pub fn len(&self) -> usize {
        self.prs.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.prs.is_empty()
    }

    /// Apply one change event. `Added`/`Updated` upsert by id (so a duplicate `Added` or an
    /// `Updated` for an unseen PR both resolve to "present with the latest data"); `Removed` drops
    /// the PR. The list stays sorted by id.
    pub fn apply(&mut self, event: ChangeEvent) {
        match event {
            ChangeEvent::Added(pr) | ChangeEvent::Updated(pr) => {
                match self.prs.iter_mut().find(|existing| existing.id == pr.id) {
                    Some(existing) => *existing = pr,
                    None => self.prs.push(pr),
                }
            }
            ChangeEvent::Removed(id) => self.prs.retain(|pr| pr.id != id),
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
}
