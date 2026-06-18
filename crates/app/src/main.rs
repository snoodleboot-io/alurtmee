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
mod splash_frames;
mod telemetry;
mod theme;
mod view;
mod widgets;
mod xdg_notifier;

use std::hash::{Hash, Hasher};
use std::time::Duration;

use directories::ProjectDirs;
use domain::{
    AuthorKind, Category, CategoryKind, ChangeEvent, Filter, PollCadence, PrId, Repo, User,
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
/// Config key holding the JSON list of configured token `(label, login)` pairs.
const CONFIG_PATS: &str = "pats";

/// Milliseconds per splash frame: 1000/12 ≈ the 12 fps source rate, i.e. original playback speed.
const SPLASH_TICK_MS: u64 = 83;
/// Number of trailing ticks over which the splash fades into the UI (~0.55s).
const SPLASH_FADE_TICKS: usize = 12;

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
    dispatcher: NotificationDispatcher<XdgNotifier>,
    focus_tx: watch::Sender<bool>,
    notifications_enabled: bool,
    /// Startup-animation tick (frame index, then fade), or `None` once the splash has finished.
    splash: Option<usize>,
}

#[derive(Debug, Clone)]
enum Message {
    PatInputChanged(String),
    LabelInputChanged(String),
    AddPatPressed,
    /// Result of validating + listing repos for the token labelled `String`.
    PatValidated(String, Result<(User, Vec<Repo>), String>),
    RemovePat(String),
    BeginRename(String),
    RenameInputChanged(String),
    CommitRename,
    CancelRename,
    ToggleRepo(String),
    PollEvent(ChangeEvent),
    CorrectCategory(PrId, CategoryKind),
    ToggleSourceFilter(AuthorKind),
    ToggleCategoryFilter(CategoryKind),
    SelectPr(PrId),
    OpenUrl(String),
    ShowSettings(bool),
    FocusChanged(bool),
    SetNotifications(bool),
    SelectSkin(String),
    SplashTick,
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
        } else {
            // Hydrate the feed from the persisted snapshot so a restart shows the current PRs
            // immediately. The poller is event-sourced from diffs, and an unchanged repo answers a
            // poll with 304 (no events), so without this the feed would be empty until something
            // actually changes — even though the cache holds every open PR.
            hydrate_feed(&store, &selection, &mut pr_list);
        }
        let selected = pr_list.prs().first().map(|pr| pr.id.clone());

        let (focus_tx, _) = watch::channel(true);

        let keychain = Keychain::new();

        // Restore the previous session. Each configured token's `(label, login)` was persisted in
        // the config DB; the secret itself is still in the OS keychain (ARD AD-6). On first launch
        // of this multi-token build, migrate a single pre-labels credential to the "default" label.
        let mut persisted = load_persisted_pats(&store);
        if persisted.is_empty() {
            if let Ok(Some(token)) = keychain.take_legacy_token() {
                if keychain.set_token("default", &token).is_ok() {
                    persisted = vec![("default".to_string(), None)];
                }
            }
        }

        let mut model = SettingsModel::new().with_selection(selection);
        model.seed_pats(persisted.iter().cloned());

        // Re-validate each configured token in the background so identities + repo lists refresh
        // and the per-token pollers resume — all with no re-entry. A revoked token surfaces as a
        // status error but is left in place for the user to remove.
        let boot_task = Task::batch(persisted.into_iter().filter_map(|(label, _)| {
            let token = keychain.get_token(&label).ok().flatten()?;
            let client = GhClient::new(base_url.clone(), token).ok()?;
            Some(validate_and_list_task(label, client))
        }));

        let app = Self {
            model,
            pr_list,
            filter: Filter::new(),
            selected,
            show_settings: false,
            skin,
            keychain,
            store,
            base_url,
            dispatcher: NotificationDispatcher::new(XdgNotifier),
            focus_tx,
            notifications_enabled,
            splash: Some(0),
        };
        (app, boot_task)
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
            Message::LabelInputChanged(value) => {
                self.model.label_input_changed(value);
                Task::none()
            }
            Message::AddPatPressed => {
                let Some((label, token)) = self.model.start_adding_pat() else {
                    return Task::none();
                };
                // Store before validating (matching the keychain-first invariant); on a build
                // failure we roll the entry back so a junk credential never lingers.
                if let Err(err) = self.keychain.set_token(&label, &token) {
                    self.model
                        .pat_failed(&label, format!("Could not store token in keychain: {err}"));
                    return Task::none();
                }
                match GhClient::new(self.base_url.clone(), token) {
                    Ok(client) => validate_and_list_task(label, client),
                    Err(err) => {
                        let _ = self.keychain.delete_token(&label);
                        self.model
                            .pat_failed(&label, format!("Could not build GitHub client: {err}"));
                        Task::none()
                    }
                }
            }
            Message::PatValidated(label, Ok((user, repos))) => {
                self.model.pat_validated(label, user, repos);
                persist_pats(&self.store, &self.model);
                Task::none()
            }
            Message::PatValidated(label, Err(reason)) => {
                self.model.pat_failed(&label, reason);
                // A brand-new add that failed (or a token we never validated) has no identity, so
                // drop its keychain entry; a known token that just failed re-validation is kept so
                // the user can see and remove it deliberately.
                let known = self
                    .model
                    .pats()
                    .iter()
                    .any(|p| p.label == label && p.login.is_some());
                if !known {
                    let _ = self.keychain.delete_token(&label);
                }
                persist_pats(&self.store, &self.model);
                Task::none()
            }
            Message::RemovePat(label) => {
                let _ = self.keychain.delete_token(&label);
                self.model.remove_pat(&label);
                persist_pats(&self.store, &self.model);
                Task::none()
            }
            Message::BeginRename(label) => {
                self.model.begin_rename(&label);
                Task::none()
            }
            Message::RenameInputChanged(value) => {
                self.model.rename_input_changed(value);
                Task::none()
            }
            Message::CancelRename => {
                self.model.cancel_rename();
                Task::none()
            }
            Message::CommitRename => {
                if let Some((old, new)) = self.model.commit_rename() {
                    // Move the secret to the new keychain account, then drop the old one.
                    if let Ok(Some(token)) = self.keychain.get_token(&old) {
                        let _ = self.keychain.set_token(&new, &token);
                        let _ = self.keychain.delete_token(&old);
                    }
                    persist_pats(&self.store, &self.model);
                }
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
                // Persist the classification verdict so the feed's tags survive a restart (the
                // poller stores PRs + enrichment itself, but emits classification only as an event).
                if let ChangeEvent::Classified(classification) = &event {
                    if let Err(err) = self.store.save_classification(classification) {
                        tracing::warn!("failed to persist classification: {err}");
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
            Message::OpenUrl(url) => {
                open_url(&url);
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
            Message::SplashTick => {
                // Advance through the frames, then the fade ticks; clear once both are done.
                let total = splash_frames::FRAMES.len() + SPLASH_FADE_TICKS;
                self.splash = match self.splash {
                    Some(tick) if tick + 1 < total => Some(tick + 1),
                    _ => None,
                };
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

        // App "chrome" subscriptions that run regardless of auth: window focus, and (while it lasts)
        // the splash-animation frame timer.
        let mut subscriptions = vec![focus];
        if self.splash.is_some() {
            subscriptions.push(
                iced::time::every(Duration::from_millis(SPLASH_TICK_MS))
                    .map(|_| Message::SplashTick),
            );
        }

        if !self.model.has_any_auth() {
            return Subscription::batch(subscriptions);
        }
        // Each watched repo is assigned to exactly one token, so we run one poller per token over a
        // disjoint repo set — a repo (and its PRs) is never polled twice even when several tokens
        // can see it.
        let assignments = self.model.poll_assignments();
        if assignments.is_empty() {
            return Subscription::batch(subscriptions);
        }

        for (label, repos) in assignments {
            let base_url = self.base_url.clone();
            let focus_rx = self.focus_tx.subscribe();
            let id = poll_subscription_id(&label, &repos);

            subscriptions.push(Subscription::run_with_id(
                id,
                iced::stream::channel(64, move |mut output| {
                    let label = label.clone();
                    let repos = repos.clone();
                    let base_url = base_url.clone();
                    let focus_rx = focus_rx.clone();
                    async move {
                        use iced::futures::SinkExt;

                        let Ok(Some(token)) = Keychain::new().get_token(&label) else {
                            tracing::warn!("poller[{label}]: no token in keychain; not polling");
                            return;
                        };
                        let client = match GhClient::new(base_url, token) {
                            Ok(client) => client,
                            Err(err) => {
                                tracing::error!("poller[{label}]: could not build client: {err}");
                                return;
                            }
                        };
                        let Some(store) = open_store_opt() else {
                            tracing::error!("poller[{label}]: could not open store");
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
            ));
        }

        Subscription::batch(subscriptions)
    }
}

// ---- plumbing -------------------------------------------------------------

/// Open a URL in the user's default browser (fire-and-forget via `xdg-open`).
fn open_url(url: &str) {
    if let Err(err) = std::process::Command::new("xdg-open").arg(url).spawn() {
        tracing::warn!("could not open {url} in a browser: {err}");
    }
}

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

fn poll_subscription_id(label: &str, repos: &[String]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "alurtmee-poller".hash(&mut hasher);
    label.hash(&mut hasher);
    repos.hash(&mut hasher);
    hasher.finish()
}

/// Fire a background task that validates the token labelled `label` and lists the repos it can see,
/// reporting back as [`Message::PatValidated`]. The token itself is captured inside `client` and
/// never travels through a `Message`.
fn validate_and_list_task(label: String, client: GhClient) -> Task<Message> {
    Task::perform(
        async move {
            let user = client.validate().await.map_err(|err| err.to_string())?;
            let repos = client
                .list_user_repos()
                .await
                .map_err(|err| err.to_string())?;
            Ok((user, repos))
        },
        move |result| Message::PatValidated(label.clone(), result),
    )
}

/// Populate `pr_list` from the cached open-PR snapshot (and enrichment) for each watched repo, so
/// the feed reflects the persisted state on launch rather than waiting for a poll diff. Per-PR
/// classification is recomputed by the poller on the next change, so it fills in over time.
fn hydrate_feed(store: &Store, selection: &domain::RepoSelection, pr_list: &mut PrListModel) {
    for repo in selection.iter() {
        let prs = match store.load_repo_prs(repo) {
            Ok(prs) => prs,
            Err(err) => {
                tracing::warn!("could not hydrate {repo} from cache: {err}");
                continue;
            }
        };
        for pr in prs {
            let id = pr.id.clone();
            pr_list.apply(ChangeEvent::Added(pr));
            if let Ok(Some(enrichment)) = store.load_enrichment(&id) {
                pr_list.apply(ChangeEvent::Enriched(enrichment));
            }
            if let Ok(Some(classification)) = store.load_classification(&id) {
                pr_list.apply(ChangeEvent::Classified(classification));
            }
        }
    }
}

/// Load the persisted `(label, login)` pairs for configured tokens from the config DB.
fn load_persisted_pats(store: &Store) -> Vec<(String, Option<String>)> {
    store
        .get_config(CONFIG_PATS)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str::<Vec<(String, String)>>(&json).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|(label, login)| (label, Some(login)))
        .collect()
}

/// Persist the validated tokens' `(label, login)` pairs (the secret stays in the keychain).
fn persist_pats(store: &Store, model: &SettingsModel) {
    match serde_json::to_string(&model.persisted_pats()) {
        Ok(json) => {
            if let Err(err) = store.set_config(CONFIG_PATS, &json) {
                tracing::error!("failed to persist token list: {err}");
            }
        }
        Err(err) => tracing::error!("failed to serialize token list: {err}"),
    }
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
