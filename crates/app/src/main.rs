//! Alurtmee desktop application entry point.
//!
//! A two-pane dashboard — a filtered master list of pull requests on the left, the selected PR's
//! detail (classification, CI status, reviews, comments) on the right, a CI-alerts strip, and a
//! Settings view for auth + repo selection. Ships five selectable dark themes (Phase 6).
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
mod xdg_notifier;

use std::hash::{Hash, Hasher};
use std::time::Duration;

use directories::ProjectDirs;
use domain::{
    AuthState, AuthorKind, Category, CategoryKind, ChangeEvent, CiAlertKind, CommentKind, Filter,
    Org, PollCadence, PrId, PullRequest, Repo, TestState, User,
};
use gh_client::GhClient;
use iced::theme::Palette;
use iced::widget::{
    button, checkbox, column, container, horizontal_space, pick_list, row, scrollable, text,
    text_input,
};
use iced::{Alignment, Border, Color, Element, Length, Subscription, Task, Theme};
use poller::Poller;
use store::{Keychain, Store};
use tokio::sync::watch;

use crate::notification_dispatcher::NotificationDispatcher;
use crate::pr_list_model::PrListModel;
use crate::settings_model::SettingsModel;
use crate::xdg_notifier::XdgNotifier;

/// Default GitHub REST base URL (override via `ALURTMEE_GITHUB_BASE_URL` for the §10 pass / tests).
const DEFAULT_GITHUB_BASE_URL: &str = "https://api.github.com";

const POLL_BASE_INTERVAL: Duration = Duration::from_secs(30);
const POLL_MAX_INTERVAL: Duration = Duration::from_secs(300);

const CONFIG_NOTIFICATIONS: &str = "notifications_enabled";
const CONFIG_THEME: &str = "theme";

const MASTER_WIDTH: f32 = 320.0;

/// A theme: a named bundle of colours. Colour means status — green good, gold important, red bad;
/// the accent is identity (selection, active filter, brand). Surfaces stay near-black.
#[derive(Clone, Copy)]
struct Skin {
    name: &'static str,
    bg: Color,
    surface: Color,
    border: Color,
    text: Color,
    muted: Color,
    accent: Color,
    accent_text: Color,
    gold: Color,
    green: Color,
    red: Color,
    slate: Color,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

const SKINS: [Skin; 5] = [
    Skin {
        name: "Aurora",
        bg: rgb(8, 11, 11),
        surface: rgb(16, 24, 22),
        border: rgb(31, 48, 45),
        text: rgb(234, 243, 239),
        muted: rgb(134, 163, 155),
        accent: rgb(47, 224, 196),
        accent_text: rgb(4, 32, 25),
        gold: rgb(243, 207, 115),
        green: rgb(95, 217, 154),
        red: rgb(247, 109, 109),
        slate: rgb(134, 163, 155),
    },
    Skin {
        name: "Velvet",
        bg: rgb(10, 8, 14),
        surface: rgb(21, 18, 29),
        border: rgb(42, 35, 56),
        text: rgb(241, 236, 247),
        muted: rgb(162, 148, 180),
        accent: rgb(207, 149, 255),
        accent_text: rgb(27, 15, 41),
        gold: rgb(240, 198, 116),
        green: rgb(122, 217, 154),
        red: rgb(247, 109, 109),
        slate: rgb(162, 148, 180),
    },
    Skin {
        name: "Synthwave",
        bg: rgb(10, 7, 16),
        surface: rgb(22, 15, 29),
        border: rgb(46, 31, 58),
        text: rgb(246, 238, 247),
        muted: rgb(169, 143, 176),
        accent: rgb(255, 69, 224),
        accent_text: rgb(33, 4, 28),
        gold: rgb(243, 207, 106),
        green: rgb(86, 224, 160),
        red: rgb(255, 93, 108),
        slate: rgb(169, 143, 176),
    },
    Skin {
        name: "Voltage",
        bg: rgb(2, 3, 1),
        surface: rgb(7, 9, 3),
        border: rgb(38, 49, 27),
        text: rgb(238, 244, 231),
        muted: rgb(139, 151, 125),
        accent: rgb(182, 255, 58),
        accent_text: rgb(22, 33, 10),
        gold: rgb(240, 197, 96),
        green: rgb(82, 217, 138),
        red: rgb(255, 93, 108),
        slate: rgb(139, 151, 125),
    },
    Skin {
        name: "Ionix",
        bg: rgb(6, 10, 14),
        surface: rgb(14, 23, 34),
        border: rgb(29, 48, 69),
        text: rgb(233, 242, 251),
        muted: rgb(128, 149, 173),
        accent: rgb(33, 230, 255),
        accent_text: rgb(3, 33, 42),
        gold: rgb(243, 207, 106),
        green: rgb(87, 224, 160),
        red: rgb(255, 93, 108),
        slate: rgb(128, 149, 173),
    },
];

/// Build the Iced theme that drives built-in widgets (inputs, checkboxes, scrollbars, pick-list).
fn iced_theme(s: &Skin) -> Theme {
    Theme::custom(
        s.name.to_string(),
        Palette {
            background: s.bg,
            text: s.text,
            primary: s.accent,
            success: s.green,
            danger: s.red,
        },
    )
}

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
            .and_then(|name| SKINS.iter().position(|s| s.name == name))
            .unwrap_or(3); // Voltage

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
                if let Some(i) = SKINS.iter().position(|s| s.name == name) {
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

    fn view(&self) -> Element<'_, Message> {
        let s = self.skin();
        let main = if self.show_settings || !self.has_feed() {
            self.settings_view(s)
        } else {
            self.feed_view(s)
        };

        let body = column![self.top_bar(s), self.ci_banner(s), main]
            .spacing(14)
            .padding(18)
            .height(Length::Fill);

        container(body)
            .style(move |_t: &Theme| container::Style {
                background: Some(s.bg.into()),
                text_color: Some(s.text),
                ..Default::default()
            })
            .into()
    }

    fn has_feed(&self) -> bool {
        self.model.auth().is_authenticated() || !self.pr_list.is_empty()
    }

    fn top_bar(&self, s: Skin) -> Element<'_, Message> {
        let signed_in = match self.model.auth() {
            AuthState::Authenticated(u) => format!("@{}", u.login),
            _ => "not signed in".to_string(),
        };
        let names: Vec<String> = SKINS.iter().map(|sk| sk.name.to_string()).collect();
        let picker = pick_list(
            names,
            Some(self.skin().name.to_string()),
            Message::SelectSkin,
        )
        .text_size(13)
        .padding([5, 10]);

        row![
            brand_dot(s),
            text("Alurtmee").size(24).color(s.text),
            text(signed_in).size(13).color(s.muted),
            horizontal_space(),
            checkbox("Notifications", self.notifications_enabled)
                .on_toggle(Message::SetNotifications)
                .size(16)
                .text_size(13),
            picker,
            ghost_button("⚙ Settings", Message::ShowSettings(!self.show_settings)),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
    }

    fn ci_banner(&self, s: Skin) -> Element<'_, Message> {
        let alerts = self.pr_list.ci_alerts();
        if alerts.is_empty() {
            return column![].into();
        }
        let failures = alerts
            .iter()
            .filter(|a| a.kind == CiAlertKind::Failure)
            .count();
        let slow = alerts.len() - failures;
        let latest = alerts
            .last()
            .map(|a| format!("latest: {} · {}", a.repo, a.reason))
            .unwrap_or_default();

        let inner = row![
            dot(s.red),
            text(format!("{failures} failed")).size(13).color(s.text),
            dot(s.gold),
            text(format!("{slow} slow")).size(13).color(s.text),
            text(latest).size(12).color(s.muted),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        container(inner)
            .padding([9, 14])
            .width(Length::Fill)
            .style(move |_t: &Theme| panel_style(s, 12.0))
            .into()
    }

    fn feed_view(&self, s: Skin) -> Element<'_, Message> {
        let master = card(
            s,
            column![
                self.filter_bar(s),
                rule(s),
                scrollable(column(self.pr_rows(s)).spacing(9).padding([0, 6])).height(Length::Fill),
            ]
            .spacing(12),
        )
        .width(Length::Fixed(MASTER_WIDTH));

        let detail = card(s, self.detail_pane(s)).width(Length::Fill);

        row![master, detail].spacing(14).height(Length::Fill).into()
    }

    fn filter_bar(&self, s: Skin) -> Element<'_, Message> {
        let shown = self.visible_prs().count();
        let total = self.pr_list.len();
        let count = if self.filter.is_active() {
            format!("{shown}/{total}")
        } else {
            format!("{total}")
        };

        column![
            row![
                text("Pull requests").size(16).color(s.text),
                horizontal_space(),
                text(count).size(13).color(s.muted)
            ]
            .align_y(Alignment::Center),
            row![
                chip(
                    s,
                    "human",
                    self.filter.is_source_active(AuthorKind::Human),
                    Message::ToggleSourceFilter(AuthorKind::Human)
                ),
                chip(
                    s,
                    "bot",
                    self.filter.is_source_active(AuthorKind::Bot),
                    Message::ToggleSourceFilter(AuthorKind::Bot)
                ),
                chip(
                    s,
                    "feature",
                    self.filter.is_category_active(CategoryKind::Feature),
                    Message::ToggleCategoryFilter(CategoryKind::Feature)
                ),
                chip(
                    s,
                    "security",
                    self.filter.is_category_active(CategoryKind::Security),
                    Message::ToggleCategoryFilter(CategoryKind::Security)
                ),
            ]
            .spacing(6),
        ]
        .spacing(10)
        .into()
    }

    fn pr_rows(&self, s: Skin) -> Vec<Element<'_, Message>> {
        let visible: Vec<&PullRequest> = self.visible_prs().collect();
        if visible.is_empty() {
            let msg = if self.pr_list.is_empty() {
                "No pull requests yet — the poller will fill this in."
            } else {
                "Nothing matches the active filters."
            };
            return vec![text(msg).size(13).color(s.muted).into()];
        }
        visible.into_iter().map(|pr| self.pr_row(s, pr)).collect()
    }

    fn pr_row(&self, s: Skin, pr: &PullRequest) -> Element<'_, Message> {
        let selected = self.selected.as_ref() == Some(&pr.id);
        let status = self
            .pr_list
            .enrichment(&pr.id)
            .map(|e| test_color(s, e.tests.state))
            .unwrap_or(s.slate);

        let is_security = self
            .pr_list
            .classification(&pr.id)
            .map(|c| c.category.kind == CategoryKind::Security)
            .unwrap_or(false);

        let mut meta = row![text(pr.id.repo.clone()).size(11).color(s.muted)].spacing(6);
        if let Some(c) = self.pr_list.classification(&pr.id) {
            meta = meta.push(pill(source_label(c.author_kind), s.slate));
            meta = meta.push(pill(
                category_label(c.category.kind),
                category_color(s, c.category.kind),
            ));
        }

        let content = column![
            row![
                dot(status),
                text(format!("#{}  {}", pr.id.number, pr.title))
                    .size(14)
                    .color(s.text),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            meta,
        ]
        .spacing(6);

        button(content)
            .on_press(Message::SelectPr(pr.id.clone()))
            .width(Length::Fill)
            .padding(10)
            .style(move |_t: &Theme, st: button::Status| {
                let hover = matches!(st, button::Status::Hovered);
                let bg = if selected {
                    mix(s.accent, s.surface, 0.16)
                } else if hover {
                    mix(s.text, s.surface, 0.05)
                } else {
                    mix(s.text, s.surface, 0.03)
                };
                let border_color = if selected {
                    mix(s.accent, s.surface, 0.65)
                } else if is_security {
                    s.gold
                } else {
                    Color::TRANSPARENT
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: s.text,
                    border: Border {
                        color: border_color,
                        width: if selected || is_security { 1.0 } else { 0.0 },
                        radius: 11.0.into(),
                    },
                    ..Default::default()
                }
            })
            .into()
    }

    fn detail_pane(&self, s: Skin) -> Element<'_, Message> {
        let Some(pr) = self
            .selected
            .as_ref()
            .and_then(|id| self.pr_list.prs().iter().find(|p| &p.id == id))
        else {
            return container(
                text("Select a pull request to see details.")
                    .size(14)
                    .color(s.muted),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        };

        let draft = if pr.draft { "  · draft" } else { "" };
        let mut detail = column![
            text(format!("{}{}", pr.title, draft))
                .size(20)
                .color(s.text),
            text(format!(
                "{}#{}  ·  @{}",
                pr.id.repo, pr.id.number, pr.author
            ))
            .size(13)
            .color(s.muted),
        ]
        .spacing(6);

        if let Some(c) = self.pr_list.classification(&pr.id) {
            detail = detail.push(
                row![
                    pill(source_label(c.author_kind), s.slate),
                    pill(
                        category_label(c.category.kind),
                        category_color(s, c.category.kind)
                    ),
                    text(format!("why: {}", c.category.signal))
                        .size(11)
                        .color(s.muted),
                    horizontal_space(),
                    ghost_button(
                        "→ feature",
                        Message::CorrectCategory(pr.id.clone(), CategoryKind::Feature)
                    ),
                    ghost_button(
                        "→ security",
                        Message::CorrectCategory(pr.id.clone(), CategoryKind::Security)
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        if let Some(e) = self.pr_list.enrichment(&pr.id) {
            detail = detail.push(rule(s));
            detail = detail.push(
                row![
                    dot(test_color(s, e.tests.state)),
                    text(test_label(e.tests.state))
                        .size(14)
                        .color(test_color(s, e.tests.state)),
                    text(format!(
                        "{} passed · {} failed · {} pending",
                        e.tests.passed, e.tests.failed, e.tests.pending
                    ))
                    .size(12)
                    .color(s.muted),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );

            detail = detail.push(
                text(format!("Reviews ({})", e.reviews.len()))
                    .size(14)
                    .color(s.text),
            );
            if e.reviews.is_empty() {
                detail = detail.push(text("  none").size(12).color(s.muted));
            }
            for review in &e.reviews {
                detail = detail.push(
                    row![
                        pill(&review.state, review_color(s, &review.state)),
                        text(format!("@{}", review.author)).size(13).color(s.text),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }

            detail = detail.push(
                text(format!("Comments ({})", e.comments.len()))
                    .size(14)
                    .color(s.text),
            );
            if e.comments.is_empty() {
                detail = detail.push(text("  none").size(12).color(s.muted));
            }
            for comment in &e.comments {
                let kind = match comment.kind {
                    CommentKind::Issue => "issue",
                    CommentKind::Review => "review",
                };
                detail = detail.push(
                    column![
                        text(format!("@{} · {kind}", comment.author))
                            .size(12)
                            .color(s.muted),
                        text(comment.body.clone()).size(13).color(s.text),
                    ]
                    .spacing(2),
                );
            }
        } else {
            detail = detail.push(
                text("Enrichment loads when the PR next changes.")
                    .size(12)
                    .color(s.muted),
            );
        }

        scrollable(detail.spacing(12)).height(Length::Fill).into()
    }

    fn settings_view(&self, s: Skin) -> Element<'_, Message> {
        let identity = match self.model.auth() {
            AuthState::Authenticated(user) => format!("Signed in as {}", user.login),
            AuthState::Invalid(reason) => format!("Not signed in — {reason}"),
            AuthState::Unauthenticated => "Not signed in".to_string(),
        };

        let validate = {
            let b = button(text("Validate"))
                .padding([8, 16])
                .style(button::primary);
            if self.model.is_busy() {
                b
            } else {
                b.on_press(Message::ValidatePressed)
            }
        };

        let mut panel = column![
            text("Settings").size(20).color(s.text),
            text(identity).size(14).color(s.muted),
            text("GitHub personal access token").size(13).color(s.text),
            text_input("ghp_…", self.model.pat_input())
                .on_input(Message::PatInputChanged)
                .secure(true)
                .padding(10),
            validate,
            text(self.model.status().to_string())
                .size(13)
                .color(s.muted),
        ]
        .spacing(10);

        if !self.model.orgs().is_empty() {
            let logins: Vec<&str> = self.model.orgs().iter().map(|o| o.login.as_str()).collect();
            panel = panel.push(
                text(format!("Organizations: {}", logins.join(", ")))
                    .size(12)
                    .color(s.muted),
            );
        }

        if !self.model.repos().is_empty() {
            panel = panel.push(rule(s));
            panel = panel.push(
                text(format!(
                    "Repositories ({} selected)",
                    self.model.selection().len()
                ))
                .size(15)
                .color(s.text),
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
            panel = panel.push(scrollable(column(repos).spacing(6)).height(Length::Fill));
        }

        if self.has_feed() {
            panel = panel.push(ghost_button("← Back to feed", Message::ShowSettings(false)));
        }

        row![
            horizontal_space(),
            card(s, panel).width(Length::Fixed(560.0)),
            horizontal_space()
        ]
        .height(Length::Fill)
        .into()
    }

    fn visible_prs(&self) -> impl Iterator<Item = &PullRequest> {
        self.pr_list
            .prs()
            .iter()
            .filter(move |pr| match self.pr_list.classification(&pr.id) {
                Some(c) => self.filter.accepts(c.author_kind, c.category.kind),
                None => true,
            })
    }
}

// ---- view helpers ---------------------------------------------------------

fn panel_style(s: Skin, radius: f32) -> container::Style {
    container::Style {
        background: Some(s.surface.into()),
        border: Border {
            color: s.border,
            width: 1.0,
            radius: radius.into(),
        },
        text_color: Some(s.text),
        ..Default::default()
    }
}

fn card<'a>(
    s: Skin,
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Container<'a, Message> {
    container(content)
        .padding(16)
        .style(move |_t: &Theme| panel_style(s, 14.0))
}

fn rule(s: Skin) -> Element<'static, Message> {
    container(horizontal_space())
        .width(Length::Fill)
        .height(1)
        .style(move |_t: &Theme| container::Style {
            background: Some(s.border.into()),
            ..Default::default()
        })
        .into()
}

fn brand_dot(s: Skin) -> Element<'static, Message> {
    container(horizontal_space())
        .width(11)
        .height(11)
        .style(move |_t: &Theme| container::Style {
            background: Some(s.accent.into()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn dot(color: Color) -> Element<'static, Message> {
    container(horizontal_space())
        .width(10)
        .height(10)
        .style(move |_t: &Theme| container::Style {
            background: Some(color.into()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 5.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn pill(label: &str, color: Color) -> Element<'static, Message> {
    let owned = label.to_string();
    container(
        text(owned)
            .size(11)
            .style(move |_t: &Theme| text::Style { color: Some(color) }),
    )
    .padding([2, 8])
    .style(move |_t: &Theme| container::Style {
        background: Some(tint(color, 0.16).into()),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 9.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn chip<'a>(s: Skin, label: &'a str, active: bool, msg: Message) -> Element<'a, Message> {
    button(text(label).size(13))
        .padding([5, 12])
        .on_press(msg)
        .style(move |_t: &Theme, st: button::Status| {
            let hover = matches!(st, button::Status::Hovered);
            let (bg, fg) = if active {
                (s.accent, s.accent_text)
            } else if hover {
                (mix(s.text, s.surface, 0.10), s.text)
            } else {
                (mix(s.text, s.surface, 0.04), s.text)
            };
            button::Style {
                background: Some(bg.into()),
                text_color: fg,
                border: Border {
                    color: if active { Color::TRANSPARENT } else { s.border },
                    width: 1.0,
                    radius: 14.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

fn ghost_button(label: &str, msg: Message) -> Element<'static, Message> {
    button(text(label.to_string()).size(13))
        .padding([5, 10])
        .on_press(msg)
        .style(button::secondary)
        .into()
}

/// Linear blend: `a` mixed into `b` by `t` (0 = all b, 1 = all a).
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: b.r + (a.r - b.r) * t,
        g: b.g + (a.g - b.g) * t,
        b: b.b + (a.b - b.b) * t,
        a: 1.0,
    }
}

fn tint(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

fn test_color(s: Skin, state: TestState) -> Color {
    match state {
        TestState::Passing => s.green,
        TestState::Failing => s.red,
        TestState::Pending => s.gold,
        TestState::None => s.slate,
    }
}

fn test_label(state: TestState) -> &'static str {
    match state {
        TestState::Passing => "tests passing",
        TestState::Failing => "tests failing",
        TestState::Pending => "tests pending",
        TestState::None => "no tests",
    }
}

fn source_label(kind: AuthorKind) -> &'static str {
    match kind {
        AuthorKind::Human => "human",
        AuthorKind::Bot => "bot",
    }
}

fn category_label(kind: CategoryKind) -> &'static str {
    match kind {
        CategoryKind::Feature => "feature",
        CategoryKind::Security => "security",
        CategoryKind::Unknown => "unknown",
    }
}

/// Security is *highlighted* (gold), not alarmed; everything else is neutral.
fn category_color(s: Skin, kind: CategoryKind) -> Color {
    match kind {
        CategoryKind::Security => s.gold,
        CategoryKind::Feature | CategoryKind::Unknown => s.slate,
    }
}

fn review_color(s: Skin, state: &str) -> Color {
    match state {
        "APPROVED" => s.green,
        "CHANGES_REQUESTED" => s.gold,
        _ => s.slate,
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
        .run_with(Alurtmee::boot)
}
