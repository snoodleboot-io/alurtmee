use std::collections::{BTreeMap, HashSet};

use domain::{Repo, RepoSelection, User};

/// One configured personal access token: a user-chosen `label`, the GitHub identity it validated to
/// (once known), and the repositories it can see. The token itself lives only in the OS keychain
/// (ARD AD-6) — never here.
#[derive(Clone, Debug)]
pub struct PatEntry {
    /// The user-chosen label that keys this token in the keychain and config.
    pub label: String,
    /// The GitHub login this token authenticates as, once validated.
    pub login: Option<String>,
    /// Repositories this token can access (from `/user/repos`), refreshed on each validate.
    pub repos: Vec<Repo>,
}

/// The pure, framework-agnostic state of the settings screen.
///
/// All transitions here are **synchronous and side-effect free** so the auth/scope flow can be
/// exercised without spinning up the Iced event loop (the network and keychain/SQLite effects are
/// driven by the Iced shell and fed back in through these methods). This split keeps the behaviour
/// unit-testable and the UI a thin render layer (SRP).
///
/// Multiple PATs are held as [`PatEntry`]s; [`SettingsModel::repos`] aggregates and **de-duplicates**
/// the repos they can see (a repo visible to several tokens appears once), and
/// [`SettingsModel::poll_assignments`] assigns each watched repo to exactly one token so its PRs are
/// never polled — or shown — twice.
///
/// No token is ever held here — only the *outcome* of validating one and the transient text the
/// user is typing. `Debug` is hand-written to redact `pat_input` so the in-flight token can never
/// leak into logs.
#[derive(Clone, Default)]
pub struct SettingsModel {
    /// Transient contents of the PAT input box. Cleared once a token is accepted; never persisted.
    pat_input: String,
    /// Transient contents of the label input box for a token being added.
    label_input: String,
    /// The configured tokens, in display / precedence order.
    pats: Vec<PatEntry>,
    /// The user's persisted choice of repositories to poll.
    selection: RepoSelection,
    /// A user-facing status / error line.
    status: String,
    /// Whether a validation or listing request is currently in flight.
    busy: bool,
}

impl std::fmt::Debug for SettingsModel {
    /// Redacts `pat_input` (the in-flight token) so the secret cannot reach logs via `{:?}`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsModel")
            .field("pat_input", &"<redacted>")
            .field("label_input", &self.label_input)
            .field("pats", &self.pats)
            .field("selection", &self.selection)
            .field("status", &self.status)
            .field("busy", &self.busy)
            .finish()
    }
}

impl SettingsModel {
    /// A fresh model with an empty selection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Restore the persisted repo selection (e.g. on application start).
    pub fn with_selection(mut self, selection: RepoSelection) -> Self {
        self.selection = selection;
        self
    }

    /// Seed the configured tokens from persisted `(label, login)` pairs so the UI shows them
    /// immediately on launch, before their background re-validation refreshes the repo lists.
    pub fn seed_pats(&mut self, pats: impl IntoIterator<Item = (String, Option<String>)>) {
        self.pats = pats
            .into_iter()
            .map(|(label, login)| PatEntry {
                label,
                login,
                repos: Vec::new(),
            })
            .collect();
    }

    // --- accessors (used by the view) ---

    /// Current PAT input text (masked when rendered).
    pub fn pat_input(&self) -> &str {
        &self.pat_input
    }

    /// Current label input text.
    pub fn label_input(&self) -> &str {
        &self.label_input
    }

    /// The configured tokens, in order.
    pub fn pats(&self) -> &[PatEntry] {
        &self.pats
    }

    /// Whether at least one token has validated (drives feed visibility and polling).
    pub fn has_any_auth(&self) -> bool {
        self.pats.iter().any(|p| p.login.is_some())
    }

    /// The de-duplicated union of repositories across all tokens, sorted by full name. A repo
    /// visible to more than one token appears exactly once (the first token, in order, wins).
    pub fn repos(&self) -> Vec<Repo> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for pat in &self.pats {
            for repo in &pat.repos {
                if seen.insert(repo.full_name.clone()) {
                    out.push(repo.clone());
                }
            }
        }
        out.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        out
    }

    /// Assign each *watched* repo to the single token that should poll it — the first token (in
    /// order) that can see it. Returns `(label, repo_full_names)` groups, so the caller spawns one
    /// poller per token over a disjoint repo set and a repo's PRs are never fetched twice.
    pub fn poll_assignments(&self) -> Vec<(String, Vec<String>)> {
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for full_name in self.selection.iter() {
            if let Some(pat) = self
                .pats
                .iter()
                .find(|p| p.repos.iter().any(|r| r.full_name == *full_name))
            {
                groups
                    .entry(pat.label.clone())
                    .or_default()
                    .push(full_name.to_string());
            }
        }
        groups.into_iter().collect()
    }

    /// The persistable `(label, login)` pairs for validated tokens (written to the config DB so the
    /// set survives a restart; the secret itself stays in the keychain).
    pub fn persisted_pats(&self) -> Vec<(String, String)> {
        self.pats
            .iter()
            .filter_map(|p| p.login.clone().map(|login| (p.label.clone(), login)))
            .collect()
    }

    /// The current persisted selection.
    pub fn selection(&self) -> &RepoSelection {
        &self.selection
    }

    /// User-facing status line.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Whether a request is in flight (drives a disabled/spinner state).
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// Whether `full_name` is currently selected for polling.
    pub fn is_selected(&self, full_name: &str) -> bool {
        self.selection.contains(full_name)
    }

    // --- transitions ---

    /// Record a change to the PAT input box.
    pub fn pat_input_changed(&mut self, value: String) {
        self.pat_input = value;
    }

    /// Record a change to the label input box.
    pub fn label_input_changed(&mut self, value: String) {
        self.label_input = value;
    }

    /// Begin adding a token. Returns the trimmed `(label, token)` to validate, or `None` if the
    /// inputs are invalid (label blank, token blank, or label already in use), in which case a
    /// status message is set. Sets the model busy so the UI can disable the button.
    pub fn start_adding_pat(&mut self) -> Option<(String, String)> {
        let label = self.label_input.trim().to_string();
        let token = self.pat_input.trim().to_string();
        if label.is_empty() {
            self.status = "Give the token a label first.".to_string();
            return None;
        }
        if token.is_empty() {
            self.status = "Paste a personal access token first.".to_string();
            return None;
        }
        if self.pats.iter().any(|p| p.label == label) {
            self.status = format!("A token labelled “{label}” already exists.");
            return None;
        }
        self.busy = true;
        self.status = format!("Validating “{label}”…");
        Some((label, token))
    }

    /// Apply a successful validation for `label`: upsert the entry with its identity and repos, and
    /// clear the input boxes (the secret is now in the keychain).
    pub fn pat_validated(&mut self, label: String, user: User, repos: Vec<Repo>) {
        let login = user.login;
        let repo_count = repos.len();
        match self.pats.iter_mut().find(|p| p.label == label) {
            Some(entry) => {
                entry.login = Some(login.clone());
                entry.repos = repos;
            }
            None => self.pats.push(PatEntry {
                label: label.clone(),
                login: Some(login.clone()),
                repos,
            }),
        }
        self.pat_input.clear();
        self.label_input.clear();
        self.busy = false;
        self.status = format!("“{label}” signed in as @{login} — {repo_count} repositories.");
    }

    /// Apply a failed validation for `label`: surface the reason and clear the busy state. A
    /// previously-seeded entry is left in place (so a transient failure does not drop a known
    /// token); the caller removes the keychain entry for a brand-new add.
    pub fn pat_failed(&mut self, label: &str, reason: impl Into<String>) {
        self.busy = false;
        self.status = format!("“{label}”: {}", reason.into());
    }

    /// Remove a configured token by label. Returns `true` if one was removed.
    pub fn remove_pat(&mut self, label: &str) -> bool {
        let before = self.pats.len();
        self.pats.retain(|p| p.label != label);
        let removed = self.pats.len() != before;
        if removed {
            self.status = format!("Removed token “{label}”.");
        }
        removed
    }

    /// Toggle a repository in/out of the selection. Returns the new selection so the caller can
    /// persist it. Slugs are `owner/name`.
    pub fn toggle_repo(&mut self, full_name: &str) -> &RepoSelection {
        self.selection.toggle(full_name.to_string());
        &self.selection
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(login: &str) -> User {
        User {
            id: 7,
            login: login.to_string(),
        }
    }

    fn repo(owner: &str, name: &str) -> Repo {
        Repo {
            id: 1,
            owner: owner.to_string(),
            name: name.to_string(),
            full_name: format!("{owner}/{name}"),
            private: false,
        }
    }

    #[test]
    fn new_model_is_unauthenticated_and_idle() {
        let model = SettingsModel::new();
        assert!(!model.is_busy());
        assert!(!model.has_any_auth());
        assert!(model.pats().is_empty());
        assert!(model.selection().is_empty());
    }

    #[test]
    fn debug_redacts_the_in_flight_token() {
        let mut model = SettingsModel::new();
        model.pat_input_changed("ghp_super_secret_token".to_string());
        let rendered = format!("{model:?}");
        assert!(!rendered.contains("ghp_super_secret_token"));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn start_adding_requires_label_and_token_and_unique_label() {
        let mut model = SettingsModel::new();
        // Blank label.
        model.pat_input_changed("ghp_x".to_string());
        assert_eq!(model.start_adding_pat(), None);
        assert!(model.status().contains("label"));
        // Blank token.
        model.pat_input_changed(String::new());
        model.label_input_changed("work".to_string());
        assert_eq!(model.start_adding_pat(), None);
        // Valid.
        model.pat_input_changed("  ghp_x  ".to_string());
        model.label_input_changed("  work  ".to_string());
        assert_eq!(
            model.start_adding_pat(),
            Some(("work".to_string(), "ghp_x".to_string())),
            "label and token are trimmed"
        );
        assert!(model.is_busy());
        // Duplicate label rejected.
        model.pat_validated("work".to_string(), user("octocat"), vec![]);
        model.label_input_changed("work".to_string());
        model.pat_input_changed("ghp_y".to_string());
        assert_eq!(model.start_adding_pat(), None);
        assert!(model.status().contains("already exists"));
    }

    #[test]
    fn pat_validated_upserts_and_clears_inputs() {
        let mut model = SettingsModel::new();
        model.label_input_changed("personal".to_string());
        model.pat_input_changed("ghp_x".to_string());
        let _ = model.start_adding_pat();
        model.pat_validated(
            "personal".to_string(),
            user("octocat"),
            vec![repo("octocat", "a")],
        );

        assert!(model.has_any_auth());
        assert_eq!(model.pats().len(), 1);
        assert_eq!(model.pats()[0].login.as_deref(), Some("octocat"));
        assert_eq!(model.pat_input(), "");
        assert_eq!(model.label_input(), "");
        assert!(!model.is_busy());
    }

    #[test]
    fn repos_are_deduped_across_tokens() {
        let mut model = SettingsModel::new();
        model.pat_validated(
            "a".to_string(),
            user("a"),
            vec![repo("org", "api"), repo("org", "web")],
        );
        model.pat_validated(
            "b".to_string(),
            user("b"),
            vec![repo("org", "api"), repo("me", "blog")],
        );

        let repos = model.repos();
        let names: Vec<&str> = repos.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(
            names,
            vec!["me/blog", "org/api", "org/web"],
            "deduped + sorted"
        );
    }

    #[test]
    fn poll_assignments_give_each_watched_repo_one_owner() {
        let mut model = SettingsModel::new();
        model.pat_validated(
            "a".to_string(),
            user("a"),
            vec![repo("org", "api"), repo("org", "web")],
        );
        model.pat_validated(
            "b".to_string(),
            user("b"),
            vec![repo("org", "api"), repo("me", "blog")],
        );
        // Watch a shared repo and a b-only repo.
        model.toggle_repo("org/api");
        model.toggle_repo("me/blog");

        let assignments: BTreeMap<String, Vec<String>> =
            model.poll_assignments().into_iter().collect();
        // org/api is shared but assigned to "a" only (first token wins) — never double-polled.
        assert_eq!(assignments.get("a"), Some(&vec!["org/api".to_string()]));
        assert_eq!(assignments.get("b"), Some(&vec!["me/blog".to_string()]));
    }

    #[test]
    fn remove_pat_drops_the_entry_and_its_repos() {
        let mut model = SettingsModel::new();
        model.pat_validated("a".to_string(), user("a"), vec![repo("org", "api")]);
        model.pat_validated("b".to_string(), user("b"), vec![repo("me", "blog")]);
        assert!(model.remove_pat("a"));
        assert_eq!(model.pats().len(), 1);
        let names: Vec<String> = model.repos().into_iter().map(|r| r.full_name).collect();
        assert_eq!(names, vec!["me/blog"], "removed token's repos are gone");
        assert!(
            !model.remove_pat("a"),
            "removing an absent label is a no-op"
        );
    }

    #[test]
    fn persisted_pats_only_includes_validated_tokens() {
        let mut model = SettingsModel::new();
        model.seed_pats([("seeded".to_string(), Some("ghost".to_string()))]);
        model.pat_validated("live".to_string(), user("octocat"), vec![]);
        let persisted: BTreeMap<String, String> = model.persisted_pats().into_iter().collect();
        assert_eq!(persisted.get("seeded"), Some(&"ghost".to_string()));
        assert_eq!(persisted.get("live"), Some(&"octocat".to_string()));
    }

    #[test]
    fn seed_pats_shows_tokens_before_validation() {
        let mut model = SettingsModel::new();
        model.seed_pats([
            ("personal".to_string(), Some("octocat".to_string())),
            ("work".to_string(), Some("octocat-work".to_string())),
        ]);
        assert_eq!(model.pats().len(), 2);
        assert!(model.has_any_auth());
        assert!(
            model.repos().is_empty(),
            "repos fill in only after re-validation"
        );
    }

    #[test]
    fn toggle_repo_flips_selection_membership() {
        let mut model = SettingsModel::new();
        assert!(!model.is_selected("acme/x"));
        model.toggle_repo("acme/x");
        assert!(model.is_selected("acme/x"));
        model.toggle_repo("acme/x");
        assert!(!model.is_selected("acme/x"));
    }

    #[test]
    fn with_selection_restores_prior_choice() {
        let restored: RepoSelection = ["acme/x".to_string(), "octocat/y".to_string()]
            .into_iter()
            .collect();
        let model = SettingsModel::new().with_selection(restored);
        assert!(model.is_selected("acme/x"));
        assert!(model.is_selected("octocat/y"));
    }
}
