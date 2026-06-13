use domain::Item;

/// A change detected during a poll cycle, emitted to the UI subscription.
///
/// The diff that produces these events lands in Phase 2; the variants define the stable contract
/// the UI will consume.
#[derive(Debug, Clone, PartialEq)]
pub enum PollEvent {
    /// An item became visible since the previous cycle.
    Added(Item),
    /// A previously seen item changed.
    Updated(Item),
    /// An item is no longer open; carries its id.
    Removed(u64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{Author, AuthorKind, Category, CategoryKind, Item};

    fn sample_item(id: u64) -> Item {
        Item {
            id,
            title: "Add dashboard filter".to_string(),
            author: Author {
                login: "octocat".to_string(),
                kind: AuthorKind::Human,
            },
            category: Category {
                kind: CategoryKind::Feature,
                confidence: 0.75,
                signal: "label".to_string(),
            },
        }
    }

    #[test]
    fn equal_added_events_compare_equal() {
        let a = PollEvent::Added(sample_item(1));
        let b = PollEvent::Added(sample_item(1));
        assert_eq!(a, b);
    }

    #[test]
    fn updated_differs_from_added_for_same_item() {
        let item = sample_item(7);
        assert_ne!(PollEvent::Added(item.clone()), PollEvent::Updated(item));
    }

    #[test]
    fn removed_events_distinguish_ids() {
        assert_eq!(PollEvent::Removed(1), PollEvent::Removed(1));
        assert_ne!(PollEvent::Removed(1), PollEvent::Removed(2));
    }
}
