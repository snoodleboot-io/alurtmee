use serde::{Deserialize, Serialize};

/// Whether an author is a human or an automated account.
///
/// Populated by the human-vs-bot classifier (AD-5) in a later phase; the type itself is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorKind {
    Human,
    Bot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn author_kind_serializes_to_snake_case() {
        let cases = [
            (AuthorKind::Human, "\"human\""),
            (AuthorKind::Bot, "\"bot\""),
        ];
        for (kind, expected) in cases {
            let json = serde_json::to_string(&kind).expect("serialize AuthorKind");
            assert_eq!(json, expected);
        }
    }
}
