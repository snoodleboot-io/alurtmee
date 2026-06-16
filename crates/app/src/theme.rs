//! Theme data and the typography/spacing constants the UI is built from.
//!
//! A [`Skin`] is a named bundle of colours; colour carries status — green good, gold important,
//! red bad — while the accent (and decorative [`Skin::accent2`]) are identity. Surfaces stay
//! near-black. The [`skin_index_by_name`] / [`skin_names`] / [`DEFAULT_SKIN_INDEX`] registry is the
//! single place that maps between a persisted theme name and its slot, so adding a skin is one edit
//! to [`SKINS`] rather than three scattered lookups.

use iced::font::{Family, Stretch, Style, Weight};
use iced::theme::Palette;
use iced::{Color, Font, Theme};

/// One UI font family, four weights — used consistently for body, headings, titles.
const fn ui_font(weight: Weight) -> Font {
    Font {
        family: Family::SansSerif,
        weight,
        stretch: Stretch::Normal,
        style: Style::Normal,
    }
}
pub(crate) const FONT_BODY: Font = ui_font(Weight::Normal);
pub(crate) const FONT_MEDIUM: Font = ui_font(Weight::Medium);
pub(crate) const FONT_SEMIBOLD: Font = ui_font(Weight::Semibold);
pub(crate) const FONT_BOLD: Font = ui_font(Weight::Bold);

/// Shared corner radius so every interactive surface matches the cards/rows.
pub(crate) const RADIUS: f32 = 10.0;

/// Fixed width of the master (PR list) pane in the two-pane feed.
pub(crate) const MASTER_WIDTH: f32 = 320.0;

/// The default skin when none is persisted: Nebula, the brand theme.
pub(crate) const DEFAULT_SKIN_INDEX: usize = 0;

/// A theme: a named bundle of colours. Colour means status — green good, gold important, red bad;
/// the accent is identity (selection, active filter, brand). Surfaces stay near-black.
#[derive(Clone, Copy)]
pub(crate) struct Skin {
    pub(crate) name: &'static str,
    pub(crate) bg: Color,
    pub(crate) surface: Color,
    pub(crate) border: Color,
    pub(crate) text: Color,
    pub(crate) muted: Color,
    pub(crate) accent: Color,
    /// Second neon — the logo's other glow (cyan against violet, etc.). Decorative
    /// identity only, never semantic. Drives the neon selection edge and accents.
    pub(crate) accent2: Color,
    pub(crate) accent_text: Color,
    pub(crate) gold: Color,
    pub(crate) green: Color,
    pub(crate) red: Color,
    pub(crate) slate: Color,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

pub(crate) const SKINS: [Skin; 6] = [
    Skin {
        // Brand theme — pulled straight from the mascot: pure-black void, electric-violet
        // glow (#8e00ff / #c31eff), cyan spark. High saturation to match the orb's energy.
        name: "Nebula",
        bg: rgb(3, 2, 8),
        surface: rgb(13, 8, 26),
        border: rgb(58, 28, 104),
        text: rgb(244, 238, 252),
        muted: rgb(159, 140, 192),
        accent: rgb(168, 56, 255),
        accent2: rgb(33, 230, 255),
        accent_text: rgb(18, 4, 38),
        gold: rgb(242, 199, 98),
        green: rgb(95, 222, 158),
        red: rgb(255, 93, 108),
        slate: rgb(159, 140, 192),
    },
    Skin {
        name: "Aurora",
        bg: rgb(8, 11, 11),
        surface: rgb(16, 24, 22),
        border: rgb(31, 48, 45),
        text: rgb(234, 243, 239),
        muted: rgb(134, 163, 155),
        accent: rgb(47, 224, 196),
        accent2: rgb(90, 208, 255),
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
        accent2: rgb(255, 121, 217),
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
        accent2: rgb(42, 212, 255),
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
        accent2: rgb(46, 230, 192),
        accent_text: rgb(22, 33, 10),
        gold: rgb(240, 197, 96),
        green: rgb(82, 217, 138),
        red: rgb(255, 93, 108),
        slate: rgb(139, 151, 125),
    },
    Skin {
        // Logo theme #2 — the orb's other half: pure black-blue void, electric-cyan
        // primary and electric-violet secondary (#b14dff). Maximum neon, both glows present.
        name: "Ionix",
        bg: rgb(3, 5, 11),
        surface: rgb(11, 16, 30),
        border: rgb(43, 38, 92),
        text: rgb(233, 242, 251),
        muted: rgb(140, 150, 190),
        accent: rgb(40, 236, 255),
        accent2: rgb(177, 77, 255),
        accent_text: rgb(2, 26, 33),
        gold: rgb(243, 207, 106),
        green: rgb(87, 224, 160),
        red: rgb(255, 93, 108),
        slate: rgb(140, 150, 190),
    },
];

/// Look up a skin slot by its persisted name, if it still exists.
pub(crate) fn skin_index_by_name(name: &str) -> Option<usize> {
    SKINS.iter().position(|s| s.name == name)
}

/// The selectable skin names, in display order (for the theme picker).
pub(crate) fn skin_names() -> Vec<String> {
    SKINS.iter().map(|s| s.name.to_string()).collect()
}

/// Build the Iced theme that drives built-in widgets (inputs, checkboxes, scrollbars, pick-list).
pub(crate) fn iced_theme(s: &Skin) -> Theme {
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
