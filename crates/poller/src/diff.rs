use std::collections::BTreeMap;

use domain::{ChangeEvent, PrId, PullRequest};

/// Diff a freshly fetched set of open PRs against the previously cached set, producing the minimal
/// list of [`ChangeEvent`]s.
///
/// A PR is **Added** if its [`PrId`] is new, **Updated** if it was seen before but its `updated_at`
/// advanced (GitHub bumps that field on any change), and **Removed** if a previously cached PR is
/// absent from the fresh set (no longer open). This is a pure function — the heart of
/// change-detection — so it is exhaustively unit-tested in isolation from any I/O.
pub fn diff_pull_requests(cached: &[PullRequest], fresh: &[PullRequest]) -> Vec<ChangeEvent> {
    let cached_by_id: BTreeMap<&PrId, &PullRequest> =
        cached.iter().map(|pr| (&pr.id, pr)).collect();
    let fresh_ids: BTreeMap<&PrId, ()> = fresh.iter().map(|pr| (&pr.id, ())).collect();

    let mut events = Vec::new();

    // Added / Updated: walk the fresh set so new and changed PRs are reported in fetch order.
    for pr in fresh {
        match cached_by_id.get(&pr.id) {
            None => events.push(ChangeEvent::Added(pr.clone())),
            Some(prev) if prev.updated_at != pr.updated_at => {
                events.push(ChangeEvent::Updated(pr.clone()))
            }
            Some(_) => {} // unchanged
        }
    }

    // Removed: cached PRs absent from the fresh set.
    for pr in cached {
        if !fresh_ids.contains_key(&pr.id) {
            events.push(ChangeEvent::Removed(pr.id.clone()));
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(repo: &str, number: u64, updated_at: &str) -> PullRequest {
        PullRequest {
            id: PrId::new(repo, number),
            title: format!("PR #{number}"),
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
    fn empty_to_populated_is_all_added() {
        let fresh = vec![pr("o/r", 1, "t1"), pr("o/r", 2, "t1")];
        let events = diff_pull_requests(&[], &fresh);
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ChangeEvent::Added(p) if p.id.number == 1));
        assert!(matches!(&events[1], ChangeEvent::Added(p) if p.id.number == 2));
    }

    #[test]
    fn identical_sets_produce_no_events() {
        let set = vec![pr("o/r", 1, "t1"), pr("o/r", 2, "t1")];
        assert!(diff_pull_requests(&set, &set).is_empty());
    }

    #[test]
    fn advanced_updated_at_is_an_update() {
        let cached = vec![pr("o/r", 1, "t1")];
        let fresh = vec![pr("o/r", 1, "t2")];
        let events = diff_pull_requests(&cached, &fresh);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ChangeEvent::Updated(p) if p.updated_at == "t2"));
    }

    #[test]
    fn absent_cached_pr_is_removed() {
        let cached = vec![pr("o/r", 1, "t1"), pr("o/r", 2, "t1")];
        let fresh = vec![pr("o/r", 1, "t1")];
        let events = diff_pull_requests(&cached, &fresh);
        assert_eq!(events, vec![ChangeEvent::Removed(PrId::new("o/r", 2))]);
    }

    #[test]
    fn mixed_add_update_remove() {
        let cached = vec![pr("o/r", 1, "t1"), pr("o/r", 2, "t1")];
        let fresh = vec![pr("o/r", 1, "t2"), pr("o/r", 3, "t1")];
        let events = diff_pull_requests(&cached, &fresh);
        // fresh order: #1 updated, #3 added; then #2 removed.
        assert_eq!(
            events,
            vec![
                ChangeEvent::Updated(pr("o/r", 1, "t2")),
                ChangeEvent::Added(pr("o/r", 3, "t1")),
                ChangeEvent::Removed(PrId::new("o/r", 2)),
            ]
        );
    }
}
