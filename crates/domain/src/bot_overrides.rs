use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Per-user overrides for human-vs-bot classification (AD-5 "user allow/deny").
///
/// Lets a user correct the heuristic for specific logins — e.g. force a `[bot]`-suffixed service
/// account that is really operated by a person to `Human`, or force a plain-looking automation
/// account to `Bot`. Overrides take precedence over the heuristic so the classifier never fights a
/// correction.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotOverrides {
    force_human: BTreeSet<String>,
    force_bot: BTreeSet<String>,
}

impl BotOverrides {
    /// An empty override set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Force `login` to classify as human.
    pub fn force_human(&mut self, login: impl Into<String>) -> &mut Self {
        self.force_human.insert(login.into());
        self
    }

    /// Force `login` to classify as bot.
    pub fn force_bot(&mut self, login: impl Into<String>) -> &mut Self {
        self.force_bot.insert(login.into());
        self
    }

    /// Whether `login` is forced to human.
    pub fn is_forced_human(&self, login: &str) -> bool {
        self.force_human.contains(login)
    }

    /// Whether `login` is forced to bot.
    pub fn is_forced_bot(&self, login: &str) -> bool {
        self.force_bot.contains(login)
    }
}
