use serde::{Deserialize, Serialize};

use crate::bot_overrides::BotOverrides;

/// Whether an author is a human or an automated account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorKind {
    Human,
    Bot,
}

impl AuthorKind {
    /// Classify an account as human or bot (AD-5).
    ///
    /// Precedence: explicit user overrides win first (so a correction is never re-overridden by the
    /// heuristic), then GitHub's account `type == "Bot"`, then the conventional `[bot]` login
    /// suffix; anything else is a human. Pure and deterministic.
    pub fn classify(login: &str, author_type: &str, overrides: &BotOverrides) -> Self {
        if overrides.is_forced_human(login) {
            return AuthorKind::Human;
        }
        if overrides.is_forced_bot(login) {
            return AuthorKind::Bot;
        }
        if author_type.eq_ignore_ascii_case("Bot") || login.ends_with("[bot]") {
            AuthorKind::Bot
        } else {
            AuthorKind::Human
        }
    }
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

    #[test]
    fn account_type_bot_is_bot_case_insensitive() {
        let o = BotOverrides::new();
        assert_eq!(AuthorKind::classify("renovate", "Bot", &o), AuthorKind::Bot);
        assert_eq!(AuthorKind::classify("renovate", "bot", &o), AuthorKind::Bot);
    }

    #[test]
    fn bot_login_suffix_is_bot() {
        let o = BotOverrides::new();
        assert_eq!(
            AuthorKind::classify("dependabot[bot]", "User", &o),
            AuthorKind::Bot
        );
    }

    #[test]
    fn plain_user_is_human() {
        let o = BotOverrides::new();
        assert_eq!(
            AuthorKind::classify("octocat", "User", &o),
            AuthorKind::Human
        );
    }

    #[test]
    fn force_human_override_beats_heuristic() {
        let mut o = BotOverrides::new();
        o.force_human("service[bot]");
        assert_eq!(
            AuthorKind::classify("service[bot]", "Bot", &o),
            AuthorKind::Human
        );
    }

    #[test]
    fn force_bot_override_beats_heuristic() {
        let mut o = BotOverrides::new();
        o.force_bot("ci-runner");
        assert_eq!(
            AuthorKind::classify("ci-runner", "User", &o),
            AuthorKind::Bot
        );
    }
}
