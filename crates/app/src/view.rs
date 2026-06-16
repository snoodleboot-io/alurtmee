//! The view layer: every `Element`-producing method of [`Alurtmee`], split out of the application
//! shell. Rust lets an `impl` block live in a submodule and still reach the parent type's private
//! fields, so the rendering code reads `self.*` exactly as before while no longer crowding `main`.

use iced::widget::{
    button, checkbox, column, container, horizontal_space, pick_list, row, scrollable, text,
    text_input,
};
use iced::{Alignment, Border, Color, Element, Length, Theme};

use domain::{AuthorKind, CategoryKind, CiAlertKind, CommentKind, PullRequest};

use crate::theme::{skin_names, Skin, FONT_BOLD, FONT_MEDIUM, FONT_SEMIBOLD, MASTER_WIDTH, RADIUS};
use crate::widgets::*;
use crate::{Alurtmee, Message};

impl Alurtmee {
    pub(crate) fn view(&self) -> Element<'_, Message> {
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
        self.model.has_any_auth() || !self.pr_list.is_empty()
    }

    fn top_bar(&self, s: Skin) -> Element<'_, Message> {
        let logins: Vec<&str> = self
            .model
            .pats()
            .iter()
            .filter_map(|p| p.login.as_deref())
            .collect();
        let signed_in = match logins.as_slice() {
            [] => "not signed in".to_string(),
            [one] => format!("@{one}"),
            many => format!("{} tokens", many.len()),
        };
        row![
            brand_mark(),
            text("Alurtmee").size(32).color(s.text).font(FONT_BOLD),
            text(signed_in).size(13).color(s.muted),
            horizontal_space(),
            checkbox("Notifications", self.notifications_enabled)
                .on_toggle(Message::SetNotifications)
                .size(16)
                .text_size(13)
                .style(checkbox_style(s)),
            ghost_button(s, "⚙ Settings", Message::ShowSettings(!self.show_settings)),
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
                text("Pull requests")
                    .size(16)
                    .color(s.text)
                    .font(FONT_SEMIBOLD),
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
                    mix(s.accent, s.surface, 0.22)
                } else if hover {
                    mix(s.accent, s.surface, 0.08)
                } else {
                    mix(s.text, s.surface, 0.03)
                };
                let border_color = if selected {
                    s.accent2
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
                        width: if selected {
                            1.5
                        } else if is_security {
                            1.0
                        } else {
                            0.0
                        },
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
                .size(21)
                .color(s.text)
                .font(FONT_SEMIBOLD),
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
                        s,
                        "→ feature",
                        Message::CorrectCategory(pr.id.clone(), CategoryKind::Feature)
                    ),
                    ghost_button(
                        s,
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
        let theme_picker = pick_list(
            skin_names(),
            Some(self.skin().name.to_string()),
            Message::SelectSkin,
        )
        .text_size(13)
        .padding([7, 12])
        .style(move |_t: &Theme, st| {
            let hover = matches!(st, pick_list::Status::Hovered | pick_list::Status::Opened);
            pick_list::Style {
                text_color: s.text,
                placeholder_color: s.muted,
                handle_color: s.accent,
                background: s.surface.into(),
                border: Border {
                    color: if hover { s.accent } else { s.border },
                    width: 1.0,
                    radius: RADIUS.into(),
                },
            }
        })
        .menu_style(move |_t: &Theme| iced::widget::overlay::menu::Style {
            background: s.surface.into(),
            border: Border {
                color: s.border,
                width: 1.0,
                radius: RADIUS.into(),
            },
            text_color: s.text,
            selected_text_color: s.accent_text,
            selected_background: s.accent.into(),
        });

        let label_input = text_input("label (e.g. work)", self.model.label_input())
            .on_input(Message::LabelInputChanged)
            .padding(11)
            .width(Length::Fixed(170.0))
            .style(input_style(s));

        let token_input = text_input("paste a PAT (ghp_…)", self.model.pat_input())
            .on_input(Message::PatInputChanged)
            .secure(true)
            .padding(11)
            .style(input_style(s));

        let add_button = primary_button(
            s,
            "Add token",
            (!self.model.is_busy()).then_some(Message::AddPatPressed),
        );

        let mut panel = column![
            text("Settings").size(22).color(s.text).font(FONT_BOLD),
            rule(s),
            text("Theme").size(15).color(s.text).font(FONT_SEMIBOLD),
            row![
                theme_picker,
                text("Pick a look — your choice is remembered.")
                    .size(12)
                    .color(s.muted),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            rule(s),
            text("GitHub tokens")
                .size(15)
                .color(s.text)
                .font(FONT_SEMIBOLD),
        ]
        .spacing(10);

        // Configured tokens. The one being renamed shows an inline edit field; the rest show their
        // identity with rename/remove buttons.
        for pat in self.model.pats() {
            let label = pat.label.clone();
            if self.model.editing_label() == Some(pat.label.as_str()) {
                let edit_field = text_input("new label", self.model.edit_input())
                    .on_input(Message::RenameInputChanged)
                    .on_submit(Message::CommitRename)
                    .padding(8)
                    .width(Length::Fixed(170.0))
                    .style(input_style(s));
                panel = panel.push(
                    row![
                        edit_field,
                        primary_button(s, "save", Some(Message::CommitRename)),
                        ghost_button(s, "cancel", Message::CancelRename),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            } else {
                let who = match &pat.login {
                    Some(login) => format!("@{login}  ·  {} repos", pat.repos.len()),
                    None => "validating…".to_string(),
                };
                panel = panel.push(
                    row![
                        text(pat.label.clone())
                            .size(13)
                            .color(s.text)
                            .font(FONT_MEDIUM),
                        text(who).size(12).color(s.muted),
                        horizontal_space(),
                        ghost_button(s, "rename", Message::BeginRename(label.clone())),
                        ghost_button(s, "remove", Message::RemovePat(label)),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                );
            }
        }

        // Add-a-token row: label + PAT + Add.
        panel = panel.push(
            row![label_input, token_input, add_button]
                .spacing(8)
                .align_y(Alignment::Center),
        );
        panel = panel.push(
            text(self.model.status().to_string())
                .size(13)
                .color(s.muted),
        );

        let repos = self.model.repos();
        if !repos.is_empty() {
            panel = panel.push(rule(s));
            panel = panel.push(
                text(format!(
                    "Repositories ({} selected)",
                    self.model.selection().len()
                ))
                .size(15)
                .color(s.text)
                .font(FONT_SEMIBOLD),
            );
            let repo_rows: Vec<Element<Message>> = repos
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
                        .style(checkbox_style(s))
                        .into()
                })
                .collect();
            panel = panel.push(scrollable(column(repo_rows).spacing(6)).height(Length::Fill));
        }

        if self.has_feed() {
            panel = panel.push(ghost_button(
                s,
                "← Back to feed",
                Message::ShowSettings(false),
            ));
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
