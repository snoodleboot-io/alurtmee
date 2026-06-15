//! Alurtmee desktop application entry point.
//!
//! Phase 1 ships the **Auth + Scope** settings screen: paste a GitHub personal access token (PAT),
//! validate it against `GET /user`, list the user's organizations and repositories, and choose a
//! subset to poll. The token is stored in the OS keychain only; the selection is persisted in
//! SQLite and restored on the next launch.
//!
//! **Why Iced's Elm/`application` model fits here (MASTER §3.6):** Alurtmee is idle most of the
//! time — it polls on a slow cadence and the UI only needs to change when a poll produces a new
//! event. Iced is retained-mode and redraws *only in response to a `Message`*, so an idle
//! dashboard costs ~no CPU between updates (NFR2). The unidirectional `state → view → message →
//! update` loop maps cleanly onto "poller emits events → state updates → widgets redraw"
//! (ARD AD-7).
//!
//! **Testability:** all auth/scope *logic* lives in [`settings_model::SettingsModel`] (pure,
//! synchronous, unit-tested) and in the `gh-client`/`store` crates (tested against a wiremock
//! GitHub server and the real keychain). This `main` is a thin shell that performs the async
//! GitHub calls and keychain/SQLite effects and feeds results back into the model — it is covered
//! by the headless window smoke test and the end-to-end acceptance test in `tests/`.

mod demo;
mod notification_dispatcher;
mod notifier;
mod pr_list_model;
mod settings_model;
mod telemetry;
mod xdg_notifier;

use std::hash::{Hash, Hasher};
use std::time::Duration;

use directories::ProjectDirs;
use domain::{
    AuthState, AuthorKind, Category, CategoryKind, ChangeEvent, CommentKind, Org, PollCadence,
    PrId, Repo, TestState, User,
};
use gh_client::GhClient;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Element, Subscription, Task};
use poller::Poller;
use store::{Keychain, Store};

use crate::notification_dispatcher::NotificationDispatcher;
use crate::pr_list_model::PrListModel;
use crate::settings_model::SettingsModel;
use crate::xdg_notifier::XdgNotifier;

/// Default GitHub REST base URL. Overridable via `ALURTMEE_GITHUB_BASE_URL` so the deferred live
/// Integration Verification pass (and tests) can point at a mock server without code changes.
const DEFAULT_GITHUB_BASE_URL: &str = "https://api.github.com";

/// Active polling interval (the cadence resets here on a change) and the backed-off idle ceiling.
const POLL_BASE_INTERVAL: Duration = Duration::from_secs(30);
const POLL_MAX_INTERVAL: Duration = Duration::from_secs(300);

/// The running application: the settings model plus the I/O collaborators it drives.
struct Alurtmee {
    model: SettingsModel,
    /// Open pull requests for the selected repos, maintained from poller change-events.
    pr_list: PrListModel,
    keychain: Keychain,
    store: Store,
    base_url: String,
    /// Built once a token is accepted; holds the token internally (redacted in `Debug`).
    client: Option<GhClient>,
    /// Fires desktop notifications for CI alerts, de-duped per (run, kind).
    dispatcher: NotificationDispatcher<XdgNotifier>,
}

/// Messages that drive state transitions.
///
/// Network results carry `String` errors (not `GhError`) so `Message` stays `Clone` — the error is
/// rendered to the user as text and never needs to be matched on structurally here.
#[derive(Debug, Clone)]
enum Message {
    PatInputChanged(String),
    ValidatePressed,
    Validated(Result<User, String>),
    Listed(Result<(Vec<Org>, Vec<Repo>), String>),
    ToggleRepo(String),
    PollEvent(ChangeEvent),
    /// User corrects a PR's category; persisted as a per-repo override and applied immediately.
    CorrectCategory(PrId, CategoryKind),
}

impl Alurtmee {
    /// Open persistent storage and restore the saved selection. Storage failures are logged and
    /// fall back to an in-memory store so the window always boots (the smoke test depends on this).
    fn boot() -> (Self, Task<Message>) {
        let store = open_store();
        let selection = store.load_selection().unwrap_or_else(|err| {
            tracing::error!("failed to load saved selection: {err}");
            Default::default()
        });
        let base_url = std::env::var("ALURTMEE_GITHUB_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_GITHUB_BASE_URL.to_string());

        // Manual UI review aid: pre-populate the dashboard with sample PRs + enrichment so the
        // detail view can be eyeballed without a token. Never active in normal runs.
        let mut pr_list = PrListModel::new();
        if std::env::var_os("ALURTMEE_DEMO").is_some() {
            for event in demo::demo_events() {
                pr_list.apply(event);
            }
        }

        let app = Self {
            model: SettingsModel::new().with_selection(selection),
            pr_list,
            keychain: Keychain::new(),
            store,
            base_url,
            client: None,
            dispatcher: NotificationDispatcher::new(XdgNotifier),
        };
        (app, Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PatInputChanged(value) => {
                self.model.pat_input_changed(value);
                Task::none()
            }
            Message::ValidatePressed => {
                let Some(token) = self.model.start_validating() else {
                    return Task::none();
                };
                // The token is written to the OS keychain here — the only place it is persisted.
                if let Err(err) = self.keychain.set_token(&token) {
                    self.model
                        .validation_failed(format!("Could not store token in keychain: {err}"));
                    return Task::none();
                }
                let client = match GhClient::new(self.base_url.clone(), token) {
                    Ok(client) => client,
                    Err(err) => {
                        self.model
                            .validation_failed(format!("Could not build GitHub client: {err}"));
                        return Task::none();
                    }
                };
                self.client = Some(client.clone());
                Task::perform(
                    async move { client.validate().await.map_err(|err| err.to_string()) },
                    Message::Validated,
                )
            }
            Message::Validated(Ok(user)) => {
                self.model.validation_succeeded(user);
                let Some(client) = self.client.clone() else {
                    return Task::none();
                };
                Task::perform(
                    async move {
                        let orgs = client.list_orgs().await.map_err(|err| err.to_string())?;
                        let repos = client
                            .list_user_repos()
                            .await
                            .map_err(|err| err.to_string())?;
                        Ok((orgs, repos))
                    },
                    Message::Listed,
                )
            }
            Message::Validated(Err(reason)) => {
                self.model.validation_failed(reason);
                Task::none()
            }
            Message::Listed(Ok((orgs, repos))) => {
                self.model.loaded_orgs(orgs);
                self.model.loaded_repos(repos);
                Task::none()
            }
            Message::Listed(Err(reason)) => {
                self.model.validation_failed(reason);
                Task::none()
            }
            Message::ToggleRepo(full_name) => {
                let selection = self.model.toggle_repo(&full_name).clone();
                if let Err(err) = self.store.save_selection(&selection) {
                    tracing::error!("failed to persist selection: {err}");
                }
                Task::none()
            }
            Message::PollEvent(event) => {
                // Fire a desktop notification for CI alerts (de-duped inside the dispatcher).
                if let ChangeEvent::CiAlert(alert) = &event {
                    self.dispatcher.dispatch(alert);
                }
                self.pr_list.apply(event);
                Task::none()
            }
            Message::CorrectCategory(id, kind) => {
                // Persist the correction (the next poll re-reads it) and reflect it immediately.
                if let Err(err) = self.store.set_correction(&id.repo, id.number, kind) {
                    tracing::error!("failed to persist correction: {err}");
                }
                self.pr_list.set_corrected_category(
                    &id,
                    Category {
                        kind,
                        confidence: 1.0,
                        signal: "correction".to_string(),
                    },
                );
                Task::none()
            }
        }
    }

    /// Run the background poller while authenticated with a non-empty selection; otherwise stay
    /// idle (no network). The subscription identity is derived from the selected repos, so changing
    /// the selection restarts the poller with the new set and signing out stops it. The poller owns
    /// its own client (token read from the keychain) and store connection, and streams change
    /// events back as [`Message::PollEvent`].
    fn subscription(&self) -> Subscription<Message> {
        if !self.model.auth().is_authenticated() {
            return Subscription::none();
        }
        let repos: Vec<String> = self.model.selection().iter().map(str::to_string).collect();
        if repos.is_empty() {
            return Subscription::none();
        }
        let base_url = self.base_url.clone();
        let id = poll_subscription_id(&repos);

        Subscription::run_with_id(
            id,
            iced::stream::channel(64, move |mut output| {
                let repos = repos.clone();
                let base_url = base_url.clone();
                async move {
                    use iced::futures::SinkExt;

                    let Ok(Some(token)) = Keychain::new().get_token() else {
                        tracing::warn!("poller: no token in keychain; not polling");
                        return;
                    };
                    let client = match GhClient::new(base_url, token) {
                        Ok(client) => client,
                        Err(err) => {
                            tracing::error!("poller: could not build client: {err}");
                            return;
                        }
                    };
                    let Some(store) = open_store_opt() else {
                        tracing::error!("poller: could not open store");
                        return;
                    };
                    let cadence = PollCadence::new(POLL_BASE_INTERVAL, POLL_MAX_INTERVAL);
                    let poller = Poller::new(client, store, cadence);

                    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
                    tokio::spawn(poller.run(repos, tx));
                    while let Some(event) = rx.recv().await {
                        if output.send(Message::PollEvent(event)).await.is_err() {
                            break; // UI dropped the subscription
                        }
                    }
                }
            }),
        )
    }

    fn view(&self) -> Element<'_, Message> {
        let pat = text_input("Personal access token", self.model.pat_input())
            .on_input(Message::PatInputChanged)
            .secure(true)
            .padding(8);

        let validate = {
            let button = button(text("Validate"));
            if self.model.is_busy() {
                button
            } else {
                button.on_press(Message::ValidatePressed)
            }
        };

        let identity = match self.model.auth() {
            AuthState::Authenticated(user) => format!("Signed in as {}", user.login),
            AuthState::Invalid(reason) => format!("Not signed in — {reason}"),
            AuthState::Unauthenticated => "Not signed in".to_string(),
        };

        let orgs_line = if self.model.orgs().is_empty() {
            String::new()
        } else {
            let logins: Vec<&str> = self
                .model
                .orgs()
                .iter()
                .map(|org| org.login.as_str())
                .collect();
            format!("Organizations: {}", logins.join(", "))
        };

        let repos: Vec<Element<Message>> = self
            .model
            .repos()
            .iter()
            .map(|repo| {
                let full_name = repo.full_name.clone();
                let checked = self.model.is_selected(&full_name);
                let label = if repo.private {
                    format!("{full_name}  (private)")
                } else {
                    full_name.clone()
                };
                checkbox(label, checked)
                    .on_toggle(move |_| Message::ToggleRepo(full_name.clone()))
                    .into()
            })
            .collect();

        let ci_alerts: Vec<Element<Message>> = self
            .pr_list
            .ci_alerts()
            .iter()
            .map(|alert| {
                let tag = match alert.kind {
                    domain::CiAlertKind::Failure => "FAILED",
                    domain::CiAlertKind::SlowCi => "SLOW",
                };
                text(format!("  [{tag}] {} · {}", alert.repo, alert.reason))
                    .size(12)
                    .into()
            })
            .collect();

        let pr_header = if self.pr_list.is_empty() {
            "Open pull requests — none yet".to_string()
        } else {
            format!("Open pull requests ({})", self.pr_list.len())
        };

        let pr_rows: Vec<Element<Message>> = self
            .pr_list
            .prs()
            .iter()
            .map(|pr| {
                let draft = if pr.draft { "  · draft" } else { "" };
                let mut card = column![text(format!(
                    "{}#{}  {}  (@{}){}",
                    pr.id.repo, pr.id.number, pr.title, pr.author, draft
                ))
                .size(14)]
                .spacing(2);

                // Classification chips (source + category) with the firing signal + correction.
                if let Some(classification) = self.pr_list.classification(&pr.id) {
                    let source = match classification.author_kind {
                        AuthorKind::Human => "human",
                        AuthorKind::Bot => "bot",
                    };
                    let category = match classification.category.kind {
                        CategoryKind::Feature => "feature",
                        CategoryKind::Security => "security",
                        CategoryKind::Unknown => "unknown",
                    };
                    card = card.push(
                        text(format!(
                            "    [{source}] [{category}]  · why: {}",
                            classification.category.signal
                        ))
                        .size(12),
                    );
                    card = card.push(
                        row![
                            text("    correct:").size(12),
                            button(text("feature").size(12)).on_press(Message::CorrectCategory(
                                pr.id.clone(),
                                CategoryKind::Feature
                            )),
                            button(text("security").size(12)).on_press(Message::CorrectCategory(
                                pr.id.clone(),
                                CategoryKind::Security
                            )),
                        ]
                        .spacing(6),
                    );
                }

                // Detail: test badge, reviews, and merged comment threads (once enriched).
                if let Some(enrichment) = self.pr_list.enrichment(&pr.id) {
                    let badge = match enrichment.tests.state {
                        TestState::Passing => "tests: passing",
                        TestState::Failing => "tests: failing",
                        TestState::Pending => "tests: pending",
                        TestState::None => "tests: none",
                    };
                    card = card.push(
                        text(format!(
                            "    {badge} (passed {}, failed {}, pending {}) · {} reviews · {} comments",
                            enrichment.tests.passed,
                            enrichment.tests.failed,
                            enrichment.tests.pending,
                            enrichment.reviews.len(),
                            enrichment.comments.len(),
                        ))
                        .size(12),
                    );
                    for review in &enrichment.reviews {
                        card = card.push(
                            text(format!("      review @{}: {}", review.author, review.state))
                                .size(12),
                        );
                    }
                    for comment in &enrichment.comments {
                        let kind = match comment.kind {
                            CommentKind::Issue => "issue",
                            CommentKind::Review => "review",
                        };
                        let preview: String = comment.body.chars().take(80).collect();
                        card = card.push(
                            text(format!("      {kind} @{}: {preview}", comment.author)).size(12),
                        );
                    }
                }

                card.into()
            })
            .collect();

        let content = column![
            text("Alurtmee — Settings").size(24),
            text(identity).size(14),
            text("GitHub personal access token").size(14),
            pat,
            validate,
            text(self.model.status().to_string()),
            text(orgs_line).size(14),
            text(format!(
                "{} repositories selected",
                self.model.selection().len()
            ))
            .size(14),
            scrollable(column(repos).spacing(4)),
            text(format!("CI alerts ({})", self.pr_list.ci_alerts().len())).size(18),
            column(ci_alerts).spacing(2),
            text(pr_header).size(18),
            scrollable(column(pr_rows).spacing(4)),
        ]
        .spacing(12)
        .padding(20);

        container(content).into()
    }
}

/// Open the persistent SQLite store under the platform data directory, falling back to an
/// in-memory store if the directory or database cannot be opened (so the window still launches).
fn open_store() -> Store {
    match open_store_opt() {
        Some(store) => store,
        None => {
            tracing::warn!("falling back to an in-memory store; selection will not persist");
            Store::open_in_memory().expect("in-memory SQLite store always opens")
        }
    }
}

/// Open the persistent store, or `None` if the data dir / database can't be opened. The poller
/// opens its own connection (SQLite permits concurrent connections to one file) via this helper.
fn open_store_opt() -> Option<Store> {
    data_db_path().and_then(|path| Store::open(&path).ok())
}

/// A stable subscription id for the poller, derived from the selected repos so the subscription is
/// recreated (poller restarted) exactly when the selection changes.
fn poll_subscription_id(repos: &[String]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "alurtmee-poller".hash(&mut hasher);
    repos.hash(&mut hasher);
    hasher.finish()
}

/// Compute the on-disk database path under the user's data directory, creating the directory.
fn data_db_path() -> Option<String> {
    let dirs = ProjectDirs::from("com", "alurtmee", "alurtmee")?;
    let data_dir = dirs.data_dir();
    std::fs::create_dir_all(data_dir).ok()?;
    data_dir
        .join("alurtmee.sqlite")
        .to_str()
        .map(str::to_string)
}

fn main() -> iced::Result {
    telemetry::init();
    tracing::info!("starting alurtmee");
    iced::application("Alurtmee", Alurtmee::update, Alurtmee::view)
        .subscription(Alurtmee::subscription)
        .run_with(Alurtmee::boot)
}
