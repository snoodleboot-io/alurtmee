//! Alurtmee desktop application entry point.
//!
//! A two-pane dashboard — a filtered master list of pull requests on the left, the selected PR's
//! detail (classification, CI status, reviews, comments) on the right, a CI-alerts strip, and a
//! Settings view for auth + repo selection. Ships six selectable dark themes.
//!
//! The shell here is deliberately thin: theme data + the skin registry live in [`theme`], reusable
//! styled widgets in [`widgets`], and every `Element`-producing method in [`view`]. This file holds
//! the `Alurtmee` state, the `Message` enum, and the Iced `boot`/`update`/`subscription` wiring.
//!
//! **Why Iced's Elm/`application` model fits here (§3.6):** the UI redraws *only on a `Message`*, so
//! an idle dashboard costs ~no CPU between poll events (NFR2). `state → view → message → update`
//! maps onto "poller emits events → state updates → widgets redraw" (AD-7).
//!
//! **Testability:** auth/scope logic lives in [`settings_model::SettingsModel`], the feed/filter in
//! [`pr_list_model::PrListModel`] + `domain::Filter`, all I/O in `gh-client`/`store`/`poller` — each
//! unit/acceptance tested. This `main` is the thin Iced shell, covered by the headless smoke test.

mod demo;
mod notification_dispatcher;
mod notifier;
mod pr_list_model;
mod settings_model;
mod telemetry;
mod theme;
mod view;
mod widgets;
mod xdg_notifier;

use std::hash::{Hash, Hasher};
use std::time::Duration;

use directories::ProjectDirs;
use domain::{
    AuthorKind, Category, CategoryKind, ChangeEvent, Filter, Org, PollCadence, PrId, Repo, User,
};
use gh_client::GhClient;
use iced::{Subscription, Task, Theme};
use poller::Poller;
use store::{Keychain, Store};
use tokio::sync::watch;

use crate::notification_dispatcher::NotificationDispatcher;
use crate::pr_list_model::PrListModel;
use crate::settings_model::SettingsModel;
use crate::theme::{iced_theme, skin_index_by_name, Skin, DEFAULT_SKIN_INDEX, FONT_BODY, SKINS};
use crate::xdg_notifier::XdgNotifier;

/// Default GitHub REST base URL (override via `ALURTMEE_GITHUB_BASE_URL` for the §10 pass / tests).
const DEFAULT_GITHUB_BASE_URL: &str = "https://api.github.com";

const POLL_BASE_INTERVAL: Duration = Duration::from_secs(30);
const POLL_MAX_INTERVAL: Duration = Duration::from_secs(300);

const CONFIG_NOTIFICATIONS: &str = "notifications_enabled";
const CONFIG_THEME: &str = "theme";

/// The running application.
struct Alurtmee {
    model: SettingsModel,
    pr_list: PrListModel,
    filter: Filter,
    selected: Option<PrId>,
    show_settings: bool,
    /// Index into [`SKINS`].
    skin: usize,
    keychain: Keychain,
    store: Store,
    base_url: String,
    client: Option<GhClient>,
    dispatcher: NotificationDispatcher<XdgNotifier>,
    focus_tx: watch::Sender<bool>,
    notifications_enabled: bool,
}

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
    SelectPr(PrId),
    ShowSettings(bool),
    FocusChanged(bool),
    SetNotifications(bool),
    SelectSkin(String),
}

impl Alurtmee {
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
        let skin = store
            .get_config(CONFIG_THEME)
            .ok()
            .flatten()
            .and_then(|name| skin_index_by_name(&name))
            .unwrap_or(DEFAULT_SKIN_INDEX);

        let mut pr_list = PrListModel::new();
        if std::env::var_os("ALURTMEE_DEMO").is_some() {
            for event in demo::demo_events() {
                pr_list.apply(event);
            }
        }
        let selected = pr_list.prs().first().map(|pr| pr.id.clone());

        let (focus_tx, _) = watch::channel(true);

        let app = Self {
            model: SettingsModel::new().with_selection(selection),
            pr_list,
            filter: Filter::new(),
            selected,
            show_settings: false,
            skin,
            keychain: Keychain::new(),
            store,
            base_url,
            client: None,
            dispatcher: NotificationDispatcher::new(XdgNotifier),
            focus_tx,
            notifications_enabled,
        };
        (app, Task::none())
    }

    fn skin(&self) -> Skin {
        SKINS[self.skin]
    }

    fn theme(&self) -> Theme {
        iced_theme(&self.skin())
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
                if self.notifications_enabled {
                    if let ChangeEvent::CiAlert(alert) = &event {
                        self.dispatcher.dispatch(alert);
                    }
                }
                self.pr_list.apply(event);
                if self.selected.is_none() {
                    self.selected = self.pr_list.prs().first().map(|pr| pr.id.clone());
                }
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
            Message::SelectPr(id) => {
                self.selected = Some(id);
                Task::none()
            }
            Message::ShowSettings(show) => {
                self.show_settings = show;
                Task::none()
            }
            Message::FocusChanged(focused) => {
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
            Message::SelectSkin(name) => {
                if let Some(i) = skin_index_by_name(&name) {
                    self.skin = i;
                    let _ = self.store.set_config(CONFIG_THEME, &name);
                }
                Task::none()
            }
        }
    }

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
                            break;
                        }
                    }
                }
            }),
        );

        Subscription::batch([focus, poller])
    }
}

// ---- plumbing -------------------------------------------------------------

fn open_store() -> Store {
    match open_store_opt() {
        Some(store) => store,
        None => {
            tracing::warn!("falling back to an in-memory store; selection will not persist");
            Store::open_in_memory().expect("in-memory SQLite store always opens")
        }
    }
}

fn open_store_opt() -> Option<Store> {
    data_db_path().and_then(|path| Store::open(&path).ok())
}

fn poll_subscription_id(repos: &[String]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "alurtmee-poller".hash(&mut hasher);
    repos.hash(&mut hasher);
    hasher.finish()
}

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
        .default_font(FONT_BODY)
        .run_with(Alurtmee::boot)
}
