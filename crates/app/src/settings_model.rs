use domain::{AuthState, Org, Repo, RepoSelection, User};

/// The pure, framework-agnostic state of the settings screen.
///
/// All transitions here are **synchronous and side-effect free** so the auth/scope flow can be
/// exercised without spinning up the Iced event loop (the network and keychain/SQLite effects are
/// driven by the Iced shell and fed back in through these methods). This split keeps the
/// behaviour unit-testable and the UI a thin render layer (SRP).
///
/// The persisted token is **never** held here — only the *outcome* of validating it
/// ([`AuthState`]) and the transient text the user is typing. The secret lives solely in the OS
/// keychain (ARD AD-6). `Debug` is hand-written to redact `pat_input` so the in-flight token can
/// never leak into logs even if the whole model is `{:?}`-printed.
#[derive(Clone, Default)]
pub struct SettingsModel {
    /// Transient contents of the PAT input box. Cleared once a token is accepted; never persisted.
    pat_input: String,
    /// Outcome of the most recent validation attempt.
    auth: AuthState,
    /// Organizations the authenticated user belongs to (for the picker).
    orgs: Vec<Org>,
    /// Repositories discoverable for the user (for the picker).
    repos: Vec<Repo>,
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
            .field("auth", &self.auth)
            .field("orgs", &self.orgs)
            .field("repos", &self.repos)
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

    // --- accessors (used by the view) ---

    /// Current PAT input text (masked when rendered).
    pub fn pat_input(&self) -> &str {
        &self.pat_input
    }

    /// The current authentication outcome.
    pub fn auth(&self) -> &AuthState {
        &self.auth
    }

    /// Discovered organizations.
    pub fn orgs(&self) -> &[Org] {
        &self.orgs
    }

    /// Discovered repositories.
    pub fn repos(&self) -> &[Repo] {
        &self.repos
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

    /// Begin a validation attempt. Returns the trimmed token to validate, or `None` if the input
    /// is blank (in which case a status message is set and no request should be issued). Sets the
    /// model to a busy state so the UI can disable the button.
    pub fn start_validating(&mut self) -> Option<String> {
        let token = self.pat_input.trim().to_string();
        if token.is_empty() {
            self.status = "Enter a personal access token first.".to_string();
            return None;
        }
        self.busy = true;
        self.status = "Validating token…".to_string();
        Some(token)
    }

    /// Apply a successful validation: record the identity and clear the input box (the secret is
    /// now safely in the keychain). Listing of orgs/repos follows as a separate step.
    pub fn validation_succeeded(&mut self, user: User) {
        self.status = format!("Authenticated as {}.", user.login);
        self.auth = AuthState::Authenticated(user);
        self.pat_input.clear();
        // Stay busy until the subsequent org/repo listing completes.
    }

    /// Apply a failed validation: surface the reason and reset to an unauthenticated, idle state.
    pub fn validation_failed(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        self.auth = AuthState::Invalid(reason.clone());
        self.orgs.clear();
        self.repos.clear();
        self.busy = false;
        self.status = reason;
    }

    /// Record the organizations discovered for the authenticated user.
    pub fn loaded_orgs(&mut self, orgs: Vec<Org>) {
        self.orgs = orgs;
    }

    /// Record the repositories discovered for the authenticated user and finish the busy state.
    pub fn loaded_repos(&mut self, repos: Vec<Repo>) {
        self.status = format!(
            "Loaded {} repositories across {} organizations.",
            repos.len(),
            self.orgs.len()
        );
        self.repos = repos;
        self.busy = false;
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

    fn user() -> User {
        User {
            id: 7,
            login: "octocat".to_string(),
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
        assert_eq!(model.auth(), &AuthState::Unauthenticated);
        assert!(model.selection().is_empty());
    }

    #[test]
    fn debug_redacts_the_in_flight_token() {
        let mut model = SettingsModel::new();
        model.pat_input_changed("ghp_super_secret_token".to_string());
        let rendered = format!("{model:?}");
        assert!(
            !rendered.contains("ghp_super_secret_token"),
            "token must not appear in Debug"
        );
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn start_validating_blank_input_sets_status_and_returns_none() {
        let mut model = SettingsModel::new();
        model.pat_input_changed("   ".to_string());
        assert_eq!(model.start_validating(), None);
        assert!(!model.is_busy());
        assert!(model.status().contains("token"));
    }

    #[test]
    fn start_validating_trims_and_returns_token_and_sets_busy() {
        let mut model = SettingsModel::new();
        model.pat_input_changed("  ghp_dummy  ".to_string());
        assert_eq!(model.start_validating(), Some("ghp_dummy".to_string()));
        assert!(model.is_busy());
    }

    #[test]
    fn validation_succeeded_records_user_and_clears_input() {
        let mut model = SettingsModel::new();
        model.pat_input_changed("ghp_dummy".to_string());
        let _ = model.start_validating();
        model.validation_succeeded(user());
        assert!(model.auth().is_authenticated());
        assert_eq!(
            model.pat_input(),
            "",
            "token text must be cleared from the input"
        );
        assert!(model.status().contains("octocat"));
    }

    #[test]
    fn validation_failed_resets_to_invalid_and_clears_lists() {
        let mut model = SettingsModel::new();
        model.pat_input_changed("bad".to_string());
        let _ = model.start_validating();
        model.loaded_orgs(vec![]);
        model.loaded_repos(vec![repo("a", "b")]);
        model.validation_failed("Token rejected (401).");
        assert_eq!(
            model.auth(),
            &AuthState::Invalid("Token rejected (401).".to_string())
        );
        assert!(model.repos().is_empty());
        assert!(!model.is_busy());
    }

    #[test]
    fn loaded_repos_finishes_busy_and_reports_count() {
        let mut model = SettingsModel::new();
        model.pat_input_changed("ghp_dummy".to_string());
        let _ = model.start_validating();
        model.validation_succeeded(user());
        model.loaded_orgs(vec![Org {
            id: 1,
            login: "acme".to_string(),
        }]);
        model.loaded_repos(vec![repo("acme", "x"), repo("octocat", "y")]);
        assert!(!model.is_busy());
        assert!(model.status().contains('2'));
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
