//! Reusable, skin-aware view widgets and the small colour/label mappers the panes share.
//!
//! These are pure presentation helpers — given a [`Skin`] they produce styled Iced elements — kept
//! out of the application shell and the per-pane views so the look-and-feel lives in one place.

use iced::widget::{button, checkbox, container, horizontal_space, image, text};
use iced::{Border, Color, Element, Length, Theme};

use domain::{AuthorKind, CategoryKind, TestState};

use crate::theme::{Skin, FONT_MEDIUM, FONT_SEMIBOLD, RADIUS};
use crate::Message;

const LOGO_BYTES: &[u8] = include_bytes!("../../../assets/logo-128.png");

pub(crate) fn panel_style(s: Skin, radius: f32) -> container::Style {
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

pub(crate) fn card<'a>(
    s: Skin,
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Container<'a, Message> {
    container(content)
        .padding(16)
        .style(move |_t: &Theme| panel_style(s, 14.0))
}

pub(crate) fn rule(s: Skin) -> Element<'static, Message> {
    container(horizontal_space())
        .width(Length::Fill)
        .height(1)
        .style(move |_t: &Theme| container::Style {
            background: Some(s.border.into()),
            ..Default::default()
        })
        .into()
}

/// The Alurtmee mascot, rendered crisply from a bundled 128px PNG.
pub(crate) fn brand_mark() -> Element<'static, Message> {
    let handle = image::Handle::from_bytes(LOGO_BYTES);
    container(image(handle).width(64).height(64))
        .width(64)
        .height(64)
        .into()
}

pub(crate) fn dot(color: Color) -> Element<'static, Message> {
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

pub(crate) fn pill(label: &str, color: Color) -> Element<'static, Message> {
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

pub(crate) fn chip<'a>(
    s: Skin,
    label: &'a str,
    active: bool,
    msg: Message,
) -> Element<'a, Message> {
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

/// Skin-colored checkbox: accent fill when checked, surface + border when not. No grey.
pub(crate) fn checkbox_style(s: Skin) -> impl Fn(&Theme, checkbox::Status) -> checkbox::Style {
    move |_t: &Theme, status: checkbox::Status| {
        let checked = matches!(
            status,
            checkbox::Status::Active { is_checked: true }
                | checkbox::Status::Hovered { is_checked: true }
        );
        checkbox::Style {
            background: if checked { s.accent } else { s.surface }.into(),
            icon_color: s.accent_text,
            border: Border {
                color: if checked { s.accent } else { s.border },
                width: 1.0,
                radius: 6.0.into(),
            },
            text_color: Some(s.text),
        }
    }
}

/// A quiet, rounded, skin-colored button — surface fill, accent on hover. No grey.
pub(crate) fn ghost_button(s: Skin, label: &str, msg: Message) -> Element<'static, Message> {
    button(text(label.to_string()).size(13).font(FONT_MEDIUM))
        .padding([6, 12])
        .on_press(msg)
        .style(move |_t: &Theme, st: button::Status| {
            let hover = matches!(st, button::Status::Hovered);
            button::Style {
                background: Some(mix(s.accent, s.surface, if hover { 0.18 } else { 0.0 }).into()),
                text_color: if hover { s.text } else { s.muted },
                border: Border {
                    color: if hover { s.accent } else { s.border },
                    width: 1.0,
                    radius: RADIUS.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

/// A filled, rounded accent button for the primary action in a view.
pub(crate) fn primary_button(
    s: Skin,
    label: &str,
    msg: Option<Message>,
) -> Element<'static, Message> {
    let mut b = button(text(label.to_string()).size(14).font(FONT_SEMIBOLD))
        .padding([9, 18])
        .style(move |_t: &Theme, st: button::Status| {
            let hover = matches!(st, button::Status::Hovered);
            button::Style {
                background: Some(
                    if hover {
                        mix(s.text, s.accent, 0.12)
                    } else {
                        s.accent
                    }
                    .into(),
                ),
                text_color: s.accent_text,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: RADIUS.into(),
                },
                ..Default::default()
            }
        });
    if let Some(m) = msg {
        b = b.on_press(m);
    }
    b.into()
}

/// Linear blend: `a` mixed into `b` by `t` (0 = all b, 1 = all a).
pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: b.r + (a.r - b.r) * t,
        g: b.g + (a.g - b.g) * t,
        b: b.b + (a.b - b.b) * t,
        a: 1.0,
    }
}

pub(crate) fn tint(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

pub(crate) fn test_color(s: Skin, state: TestState) -> Color {
    match state {
        TestState::Passing => s.green,
        TestState::Failing => s.red,
        TestState::Pending => s.gold,
        TestState::None => s.slate,
    }
}

pub(crate) fn test_label(state: TestState) -> &'static str {
    match state {
        TestState::Passing => "tests passing",
        TestState::Failing => "tests failing",
        TestState::Pending => "tests pending",
        TestState::None => "no tests",
    }
}

pub(crate) fn source_label(kind: AuthorKind) -> &'static str {
    match kind {
        AuthorKind::Human => "human",
        AuthorKind::Bot => "bot",
    }
}

pub(crate) fn category_label(kind: CategoryKind) -> &'static str {
    match kind {
        CategoryKind::Feature => "feature",
        CategoryKind::Security => "security",
        CategoryKind::Unknown => "unknown",
    }
}

/// Security is *highlighted* (gold), not alarmed; everything else is neutral.
pub(crate) fn category_color(s: Skin, kind: CategoryKind) -> Color {
    match kind {
        CategoryKind::Security => s.gold,
        CategoryKind::Feature | CategoryKind::Unknown => s.slate,
    }
}

pub(crate) fn review_color(s: Skin, state: &str) -> Color {
    match state {
        "APPROVED" => s.green,
        "CHANGES_REQUESTED" => s.gold,
        _ => s.slate,
    }
}
