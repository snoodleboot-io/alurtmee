//! Alurtmee desktop application entry point.
//!
//! The window is a single dashboard: a Settings panel (PAT → keychain, repo selection), a filter
//! bar of toggle-chips, a CI-alerts strip, and the live pull-request feed with classification chips
//! and enrichment detail. Theming, layout, and styling land in Phase 6.
//!
//! **Why Iced's Elm/`application` model fits here (MASTER §3.6):** Alurtmee is idle most of the
//! time — it polls on a slow cadence and the UI only redraws *in response to a `Message`*, so an
//! idle dashboard costs ~no CPU between updates (NFR2). The unidirectional `state → view → message
//! → update` loop maps cleanly onto "poller emits events → state updates → widgets redraw" (AD-7).
//!
//! **Testability:** auth/scope logic lives in [`settings_model::SettingsModel`], the feed/filter in
//! [`pr_list_model::PrListModel`] + `domain::Filter`, and all I/O in the `gh-client`/`store`/
//! `poller` crates — each unit/acceptance tested. This `main` is the thin Iced shell, covered by the
//! headless window smoke test.

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
    AuthState, AuthorKind, Category, CategoryKind, ChangeEvent, CiAlertKind, CommentKind, Filter,
    Org, PollCadence, PrId, Repo, TestState, User,
};
use gh_client::GhClient;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Color, Element, Subscription, Task, Theme};
use poller::Poller;
use store::{Keychain, Store};
use tokio::sync::watch;

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

/// Config keys for persisted UI preferences.
const CONFIG_NOTIFICATIONS: &str = "notifications_enabled";
const CONFIG_THEME: &str = "theme";

// Badge colours (work on the dark theme; readable on light too).
const GREEN: Color = Color::from_rgb(0.30, 0.78, 0.45);
const RED: Color = Color::from_rgb(0.90, 0.35, 0.35);
const AMBER: Color = Color::from_rgb(0.92, 0.70, 0.25);
const BLUE: Color = Color::from_rgb(0.40, 0.62, 0.95);
const GREY: Color = Color::from_rgb(0.60, 0.60, 0.64);

/// The running application: the settings/feed models plus the I/O collaborators they drive.
struct Alurtmee {
    model: SettingsModel,
    pr_list: PrListModel,
    /// Composable feed filter (source × category chips).
    filter: Filter,
    keychain: Keychain,
    store: Store,
    base_url: String,
    /// Built once a token is accepted; holds the token internally (redacted in `Debug`).
    client: Option<GhClient>,
    /// Fires desktop notifications for CI alerts, de-duped per (run, kind).
    dispatcher: NotificationDispatcher<XdgNotifier>,
    /// Window-focus signal handed to the running poller so it backs off when blurred.
    focus_tx: watch::Sender<bool>,
    notifications_enabled: bool,
    dark_theme: bool,
}

/// Messages that drive state transitions.
#[derive(Debug, Clone)]
enum Message {
    PatInputChanged(String),
    ValidatePressed,
    Validated(Result<User, String>),
    Listed(Result<(Vec<Org>, Vec<Repo>), String>),
    ToggleRepo(String),
    PollEvent(ChangeEvent),
    CorrectCategory(PrId, CategoryKind),
    ToggleSourceFilter(AuthorKind),
    ToggleCategoryFilter(CategoryKind),
    FocusChanged(bool),
    SetNotifications(bool),
    ToggleTheme,
}

impl Alurtmee {
    /// Open persistent storage, restore the saved selection + UI preferences, and (in demo mode)
    /// seed sample feed data. Storage failures fall back to in-memory so the window always boots.
    fn boot() -> (Self, Task<Message>) {
        let store = open_store();
        let selection = store.load_selection().unwrap_or_else(|err| {
            tracing::error!("failed to load saved selection: {err}");
            Default::default()
        });
        let base_url = std::env::var("ALURTMEE_GITHUB_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_GITHUB_BASE_URL.to_string());
        let notifications_enabled = store
            .get_config(CONFIG_NOTIFICATIONS)
            .ok()
            .flatten()
            .map(|v| v != "false")
            .unwrap_or(true);
        let dark_theme = store
            .get_config(CONFIG_THEME)
            .ok()
            .flatten()
            .map(|v| v != "light")
            .unwrap_or(true);

        // Manual UI review aid: pre-populate the feed so the dashboard can be eyeballed without a
        // token. Never active in normal runs.
        let mut pr_list = PrListModel::new();
        if std::env::var_os("ALURTMEE_DEMO").is_some() {
            for event in demo::demo_events() {
                pr_list.apply(event);
            }
        }

        let (focus_tx, _) = watch::channel(true);

        let app = Self {
            model: SettingsModel::new().with_selection(selection),
            pr_list,
            filter: Filter::new(),
            keychain: Keychain::new(),
            store,
            base_url,
            client: None,
            dispatcher: NotificationDispatcher::new(XdgNotifier),
            focus_tx,
            notifications_enabled,
            dark_theme,
        };
        (app, Task::none())
    }

    fn theme(&self) -> Theme {
        if self.dark_theme {
            Theme::Dark
        } else {
            Theme::Light
        }
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
                // Fire a desktop notification for CI alerts (when enabled; de-duped in dispatcher).
                if self.notifications_enabled {
                    if let ChangeEvent::CiAlert(alert) = &event {
                        self.dispatcher.dispatch(alert);
                    }
                }
                self.pr_list.apply(event);
                Task::none()
            }
            Message::CorrectCategory(id, kind) => {
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
            Message::ToggleSourceFilter(source) => {
                self.filter.toggle_source(source);
                Task::none()
            }
            Message::ToggleCategoryFilter(category) => {
                self.filter.toggle_category(category);
                Task::none()
            }
            Message::FocusChanged(focused) => {
                // Hand the new focus state to the running poller (it reads this each cycle).
                let _ = self.focus_tx.send(focused);
                Task::none()
            }
            Message::SetNotifications(enabled) => {
                self.notifications_enabled = enabled;
                let _ = self
                    .store
                    .set_config(CONFIG_NOTIFICATIONS, if enabled { "true" } else { "false" });
                Task::none()
            }
            Message::ToggleTheme => {
                self.dark_theme = !self.dark_theme;
                let _ = self
                    .store
                    .set_config(CONFIG_THEME, if self.dark_theme { "dark" } else { "light" });
                Task::none()
            }
        }
    }

    /// Subscriptions: always listen for window focus/blur; additionally run the poller when
    /// authenticated with a non-empty selection. The poller receives the focus signal so it backs
    /// off while the window is blurred (NFR2).
    fn subscription(&self) -> Subscription<Message> {
        let focus = iced::event::listen_with(|event, _status, _id| match event {
            iced::Event::Window(iced::window::Event::Focused) => Some(Message::FocusChanged(true)),
            iced::Event::Window(iced::window::Event::Unfocused) => {
                Some(Message::FocusChanged(false))
            }
            _ => None,
        });

        if !self.model.auth().is_authenticated() {
            return focus;
        }
        let repos: Vec<String> = self.model.selection().iter().map(str::to_string).collect();
        if repos.is_empty() {
            return focus;
        }
        let base_url = self.base_url.clone();
        let focus_rx = self.focus_tx.subscribe();
        let id = poll_subscription_id(&repos);

        let poller = Subscription::run_with_id(
            id,
            iced::stream::channel(64, move |mut output| {
                let repos = repos.clone();
                let base_url = base_url.clone();
                let focus_rx = focus_rx.clone();
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
                    tokio::spawn(poller.run(repos, tx, focus_rx));
                    while let Some(event) = rx.recv().await {
                        if output.send(Message::PollEvent(event)).await.is_err() {
                            break; // UI dropped the subscription
                        }
                    }
                }
            }),
        );

        Subscription::batch([focus, poller])
    }

    fn view(&self) -> Element<'_, Message> {
        let content = column![
            self.header(),
            self.settings_panel(),
            self.filter_bar(),
            self.ci_alerts_panel(),
            self.feed_panel(),
        ]
        .spacing(16)
        .padding(20);

        container(scrollable(content)).into()
    }

    /// Title row + theme / notifications toggles.
    fn header(&self) -> Element<'_, Message> {
        let theme_label = if self.dark_theme {
            "◐ Dark"
        } else {
            "◑ Light"
        };
        row![
            text("Alurtmee").size(28),
            iced::widget::horizontal_space(),
            checkbox("Notifications", self.notifications_enabled)
                .on_toggle(Message::SetNotifications),
            button(text(theme_label).size(13))
                .on_press(Message::ToggleTheme)
                .style(button::secondary),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Auth + repo-selection settings, in a card.
    fn settings_panel(&self) -> Element<'_, Message> {
        let identity = match self.model.auth() {
            AuthState::Authenticated(user) => format!("Signed in as {}", user.login),
            AuthState::Invalid(reason) => format!("Not signed in — {reason}"),
            AuthState::Unauthenticated => "Not signed in".to_string(),
        };

        let pat = text_input("Personal access token", self.model.pat_input())
            .on_input(Message::PatInputChanged)
            .secure(true)
            .padding(8);

        let validate = {
            let b = button(text("Validate")).style(button::primary);
            if self.model.is_busy() {
                b
            } else {
                b.on_press(Message::ValidatePressed)
            }
        };

        let mut panel = column![
            text(identity).size(15),
            text("GitHub personal access token").size(13),
            pat,
            validate,
            text(self.model.status().to_string()).size(13),
        ]
        .spacing(8);

        if !self.model.orgs().is_empty() {
            let logins: Vec<&str> = self.model.orgs().iter().map(|o| o.login.as_str()).collect();
            panel = panel.push(text(format!("Organizations: {}", logins.join(", "))).size(13));
        }

        if !self.model.repos().is_empty() {
            panel = panel.push(
                text(format!(
                    "Repositories ({} selected)",
                    self.model.selection().len()
                ))
                .size(14),
            );
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
            panel = panel.push(scrollable(column(repos).spacing(4)).height(140));
        }

        card(panel.into())
    }

    /// Source × category filter chips with a live "showing N of M" count.
    fn filter_bar(&self) -> Element<'_, Message> {
        let sources = row![
            text("Source:").size(13),
            chip(
                "human",
                self.filter.is_source_active(AuthorKind::Human),
                Message::ToggleSourceFilter(AuthorKind::Human)
            ),
            chip(
                "bot",
                self.filter.is_source_active(AuthorKind::Bot),
                Message::ToggleSourceFilter(AuthorKind::Bot)
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let categories = row![
            text("Kind:").size(13),
            chip(
                "feature",
                self.filter.is_category_active(CategoryKind::Feature),
                Message::ToggleCategoryFilter(CategoryKind::Feature)
            ),
            chip(
                "security",
                self.filter.is_category_active(CategoryKind::Security),
                Message::ToggleCategoryFilter(CategoryKind::Security)
            ),
            chip(
                "unknown",
                self.filter.is_category_active(CategoryKind::Unknown),
                Message::ToggleCategoryFilter(CategoryKind::Unknown)
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let shown = self.visible_prs().count();
        let total = self.pr_list.len();
        let summary = if self.filter.is_active() {
            format!("showing {shown} of {total}")
        } else {
            format!("{total} pull requests")
        };

        card(
            column![
                row![
                    text("Filters").size(16),
                    iced::widget::horizontal_space(),
                    text(summary).size(13)
                ]
                .align_y(iced::Alignment::Center),
                sources,
                categories,
            ]
            .spacing(8)
            .into(),
        )
    }

    /// CI alerts strip (failures / slow runs), coloured.
    fn ci_alerts_panel(&self) -> Element<'_, Message> {
        let alerts = self.pr_list.ci_alerts();
        let header = text(format!("CI alerts ({})", alerts.len())).size(16);
        if alerts.is_empty() {
            return card(
                column![header, text("No CI alerts.").size(13).style(grey)]
                    .spacing(6)
                    .into(),
            );
        }
        let rows: Vec<Element<Message>> = alerts
            .iter()
            .map(|alert| {
                let (tag, color) = match alert.kind {
                    CiAlertKind::Failure => ("FAILED", RED),
                    CiAlertKind::SlowCi => ("SLOW", AMBER),
                };
                row![
                    badge(tag, color),
                    text(format!("{} · {}", alert.repo, alert.reason)).size(13),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center)
                .into()
            })
            .collect();
        card(column![header, column(rows).spacing(6)].spacing(8).into())
    }

    /// The pull-request feed (filtered), each PR a styled card. Renders empty/auth states.
    fn feed_panel(&self) -> Element<'_, Message> {
        if !self.model.auth().is_authenticated() {
            return card(
                text("Validate a token and select repositories to start watching pull requests.")
                    .size(14)
                    .style(grey)
                    .into(),
            );
        }
        if self.model.selection().is_empty() {
            return card(
                text("No repositories selected — pick some above to populate the feed.")
                    .size(14)
                    .style(grey)
                    .into(),
            );
        }

        let visible: Vec<Element<Message>> =
            self.visible_prs().map(|pr| self.pr_card(pr)).collect();
        let header = text(format!("Open pull requests ({})", visible.len())).size(18);

        if visible.is_empty() {
            let msg = if self.pr_list.is_empty() {
                "No open pull requests yet — the poller will fill this in."
            } else {
                "No pull requests match the active filters."
            };
            return column![header, card(text(msg).size(14).style(grey).into())]
                .spacing(8)
                .into();
        }

        column![header, column(visible).spacing(10)]
            .spacing(8)
            .into()
    }

    /// PRs passing the active filter (unclassified PRs are always shown).
    fn visible_prs(&self) -> impl Iterator<Item = &domain::PullRequest> {
        self.pr_list
            .prs()
            .iter()
            .filter(move |pr| match self.pr_list.classification(&pr.id) {
                Some(c) => self.filter.accepts(c.author_kind, c.category.kind),
                None => true,
            })
    }

    /// One pull request rendered as a card: title, classification chips + correction, enrichment.
    fn pr_card(&self, pr: &domain::PullRequest) -> Element<'_, Message> {
        let draft = if pr.draft { "  · draft" } else { "" };
        let mut body = column![text(format!(
            "{}#{}  {}{}",
            pr.id.repo, pr.id.number, pr.title, draft
        ))
        .size(15)]
        .spacing(6);

        body = body.push(text(format!("@{}", pr.author)).size(12).style(grey));

        if let Some(classification) = self.pr_list.classification(&pr.id) {
            let (src_label, src_color) = match classification.author_kind {
                AuthorKind::Human => ("human", BLUE),
                AuthorKind::Bot => ("bot", GREY),
            };
            let (cat_label, cat_color) = match classification.category.kind {
                CategoryKind::Feature => ("feature", BLUE),
                CategoryKind::Security => ("security", RED),
                CategoryKind::Unknown => ("unknown", GREY),
            };
            body = body.push(
                row![
                    badge(src_label, src_color),
                    badge(cat_label, cat_color),
                    text(format!("why: {}", classification.category.signal))
                        .size(11)
                        .style(grey),
                    iced::widget::horizontal_space(),
                    button(text("→feature").size(11))
                        .on_press(Message::CorrectCategory(
                            pr.id.clone(),
                            CategoryKind::Feature
                        ))
                        .style(button::secondary),
                    button(text("→security").size(11))
                        .on_press(Message::CorrectCategory(
                            pr.id.clone(),
                            CategoryKind::Security
                        ))
                        .style(button::secondary),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            );
        }

        if let Some(enrichment) = self.pr_list.enrichment(&pr.id) {
            let (badge_label, badge_color) = match enrichment.tests.state {
                TestState::Passing => ("tests passing", GREEN),
                TestState::Failing => ("tests failing", RED),
                TestState::Pending => ("tests pending", AMBER),
                TestState::None => ("no tests", GREY),
            };
            body = body.push(
                row![
                    badge(badge_label, badge_color),
                    text(format!(
                        "passed {} · failed {} · pending {} · {} reviews · {} comments",
                        enrichment.tests.passed,
                        enrichment.tests.failed,
                        enrichment.tests.pending,
                        enrichment.reviews.len(),
                        enrichment.comments.len(),
                    ))
                    .size(12)
                    .style(grey),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
            for review in &enrichment.reviews {
                body = body
                    .push(text(format!("  review @{}: {}", review.author, review.state)).size(12));
            }
            for comment in &enrichment.comments {
                let kind = match comment.kind {
                    CommentKind::Issue => "issue",
                    CommentKind::Review => "review",
                };
                let preview: String = comment.body.chars().take(80).collect();
                body = body.push(text(format!("  {kind} @{}: {preview}", comment.author)).size(12));
            }
        }

        card(body.into())
    }
}

/// Wrap content in a rounded, padded card.
fn card(content: Element<'_, Message>) -> Element<'_, Message> {
    container(content)
        .style(container::rounded_box)
        .padding(14)
        .width(iced::Length::Fill)
        .into()
}

/// A small coloured badge.
fn badge(label: &str, color: Color) -> Element<'static, Message> {
    text(label.to_uppercase())
        .size(11)
        .style(move |_theme: &Theme| text::Style { color: Some(color) })
        .into()
}

/// Grey secondary text style.
fn grey(_theme: &Theme) -> text::Style {
    text::Style { color: Some(GREY) }
}

/// A filter toggle-chip: primary when active, secondary when not.
fn chip<'a>(label: &'a str, active: bool, msg: Message) -> Element<'a, Message> {
    let b = button(text(label).size(12)).padding([4, 10]).on_press(msg);
    if active {
        b.style(button::primary).into()
    } else {
        b.style(button::secondary).into()
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
        .theme(Alurtmee::theme)
        .run_with(Alurtmee::boot)
}
