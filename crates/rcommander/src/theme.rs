//! The Fluent-like look: Windows 11's colour tokens for light and dark, its fonts, and the style
//! functions the views use.
//!
//! The colours follow WinUI's theme resources (`SolidBackgroundFillColorBase`, the card and
//! stroke fills, `TextFillColorPrimary/Secondary`, the default accent). iced reports the system's
//! light or dark mode ([`iced::system::theme`]); the user's accent colour comes from the registry
//! on Windows (`system.rs`), Windows' default blue elsewhere. The setting `appearance.theme`
//! chooses light, dark or the system's look ([`ThemeChoice`]).

mod chrome;
pub mod colour_rules;
pub mod colours;
mod dark;
pub mod ellipsis;
pub mod fit;
pub mod focus;
pub mod fonts;
pub mod icon;
pub mod layout;
mod light;
#[cfg(target_os = "macos")]
mod mac;
pub mod mnemonic;
pub mod page_tabs;
mod syntax;
mod system;
pub mod wheel;

use std::sync::OnceLock;

pub use chrome::{
    alternate, drive_line, drive_line_rule, header_cell, set_stripes, striped_row, tab,
};
pub use layout::large_dialog;
pub use syntax::Syntax;

use iced::{
    Background, Border, Color, Font, Shadow, Theme,
    font::Weight,
    theme::Palette,
    widget::{button, container},
};

/// The colours of one appearance.
#[derive(Debug)]
pub struct Tokens {
    pub dark: bool,
    /// The window behind the panels.
    pub window: Color,
    /// A panel's body.
    pub surface: Color,
    /// The path header and the column header.
    pub header: Color,
    pub stroke: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub accent: Color,
    /// The cursor row of the active panel.
    pub cursor_fill: Color,
    /// The cursor row of the inactive panel.
    pub cursor_fill_inactive: Color,
    /// A selected (marked) row.
    pub selected_fill: Color,
    /// Every other row of a list or a table, while `appearance.alternate_rows` is on; the cursor
    /// and the selection always win over it.
    pub stripe_fill: Color,
    /// A function-key button.
    pub control: Color,
    /// The file lists' and the panels' fill: `surface` unless the person chose another
    /// (RC-100-43).
    pub list_fill: Color,
    /// The text in the lists and the panels: `text` unless the person chose another.
    pub list_text: Color,
    /// The lists' quieter text - sizes, dates: `text_secondary`, or the chosen text softened.
    pub list_text_secondary: Color,
    /// The text of a cursor row.
    pub cursor_text: Color,
    /// A marked entry's name, as Total Commander colours it: the accent unless chosen.
    pub marked: Color,
    /// A plugin document's coloured text, per token class (RC-100-10).
    pub syntax: Syntax,
}

const fn hex(rgb: u32, alpha: f32) -> Color {
    Color {
        r: ((rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: (rgb & 0xff) as f32 / 255.0,
        a: alpha,
    }
}

static TOKENS: OnceLock<[Tokens; 2]> = OnceLock::new();
/// The system's accent for the light and the dark look, read once.
static ACCENT: OnceLock<system::Accent> = OnceLock::new();

/// The system's accent for the light and the dark look, as [`tokens`] draws with it unless the
/// person chose another (RC-100-43).
pub fn system_accent() -> [Color; 2] {
    let accent = ACCENT.get_or_init(system::accent);
    [accent.light, accent.dark]
}

/// Reads the system's accent colour; the start calls it before the window opens, so drawing
/// never touches the registry. Later calls change nothing.
pub fn init() {
    tokens(false);
}

/// The tokens of the light or the dark look, with the system's accent; with the person's colours
/// laid over them while any is chosen ([`colours::set_colours`]).
pub fn tokens(dark: bool) -> &'static Tokens {
    if let Some(chosen) = colours::chosen(dark) {
        return chosen;
    }
    let [light, dark_tokens] = TOKENS.get_or_init(|| {
        let [light, dark] = system_accent();
        if cfg!(target_os = "macos") {
            mac_tokens(light, dark)
        } else {
            [light::tokens(light), dark::tokens(dark)]
        }
    });
    if dark { dark_tokens } else { light }
}

/// The Mac's light and dark tokens; WinUI's on other systems.
#[cfg(target_os = "macos")]
fn mac_tokens(light: Color, dark: Color) -> [Tokens; 2] {
    [mac::light(light), mac::dark(dark)]
}

#[cfg(not(target_os = "macos"))]
fn mac_tokens(light: Color, dark: Color) -> [Tokens; 2] {
    [light::tokens(light), dark::tokens(dark)]
}

/// The look the setting `appearance.theme` chooses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    /// Light or dark as Windows is, following a change while the app runs.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    /// The choice a setting's value names; the default for anything else.
    pub fn from_setting(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }

    /// Whether the window is dark, given whether the system is.
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            Self::System => system_dark,
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

/// An error, or a warning that something is lost for good: text, never a button's fill.
pub const DANGER: Color = hex(0xC42B1C, 1.0);

/// The iced theme the built-in widgets draw with, from the same tokens.
pub fn iced_theme(tokens: &Tokens) -> Theme {
    let palette = Palette {
        background: tokens.window,
        text: tokens.text,
        primary: tokens.accent,
        success: hex(0x0F7B0F, 1.0),
        warning: hex(0x9D5D00, 1.0),
        danger: DANGER,
    };
    let name = if tokens.dark {
        "Fluent Dark"
    } else {
        "Fluent Light"
    };
    Theme::custom(name, palette)
}

/// The interface font: Segoe UI Variable on Windows 11, the system's sans-serif elsewhere (and
/// on Windows 10, where the name does not resolve and cosmic-text falls back to Segoe UI).
pub const UI_FONT: Font = if cfg!(windows) {
    Font::with_name("Segoe UI Variable")
} else if cfg!(target_os = "macos") {
    // San Francisco, the Mac's interface font (/System/Library/Fonts/SFNS.ttf)
    Font::with_name("System Font")
} else {
    Font::DEFAULT
};

/// The interface font, semibold.
pub const UI_FONT_STRONG: Font = Font {
    weight: Weight::Semibold,
    ..UI_FONT
};

/// The monospace font: Cascadia Mono (Windows 11, Windows Terminal) where present; the
/// terminal's and the viewer's until another is chosen ([`fonts`]).
pub const MONO_FONT: Font = if cfg!(windows) {
    Font::with_name("Cascadia Mono")
} else if cfg!(target_os = "macos") {
    Font::with_name("Menlo")
} else {
    Font::MONOSPACE
};

pub const TEXT_SIZE: f32 = 13.0;
/// The status lines' text, a size under [`TEXT_SIZE`]: a panel's, the window's and what they
/// offer to click (RC-080-53).
pub const STATUS_SIZE: f32 = 12.0;
/// A dialog table's row; a file list's follows its chosen font ([`fonts::AreaFont::row_height`]).
pub const ROW_HEIGHT: f32 = 24.0;
const RADIUS: f32 = 6.0;

pub fn window(tokens: &Tokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.window)),
        text_color: Some(tokens.text),
        ..container::Style::default()
    }
}

/// A panel's card: the active one carries an accent border and a soft shadow.
pub fn panel(tokens: &Tokens, active: bool) -> container::Style {
    let (border, shadow) = if active {
        (
            tokens.accent,
            Shadow {
                color: hex(0x000000, if tokens.dark { 0.35 } else { 0.10 }),
                offset: iced::Vector::new(0.0, 2.0),
                blur_radius: 8.0,
            },
        )
    } else {
        (tokens.stroke, Shadow::default())
    };
    container::Style {
        background: Some(Background::Color(tokens.list_fill)),
        border: Border::default().rounded(RADIUS).width(1.0).color(border),
        shadow,
        text_color: Some(tokens.list_text),
        snap: true,
    }
}

/// The path header strip at the top of a panel.
pub fn header(tokens: &Tokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.header)),
        border: Border::default().rounded(iced::border::Radius {
            top_left: RADIUS,
            top_right: RADIUS,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }),
        ..container::Style::default()
    }
}

/// A one-pixel line.
pub fn divider(tokens: &Tokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.stroke)),
        ..container::Style::default()
    }
}

/// A list row: the cursor row is filled (accent-tinted in the active panel), a marked row tinted.
pub fn row(tokens: &Tokens, cursor: bool, active: bool, selected: bool) -> container::Style {
    let fill = match (cursor, active, selected) {
        (true, true, _) => Some(tokens.cursor_fill),
        (true, false, _) => Some(tokens.cursor_fill_inactive),
        (false, _, true) => Some(tokens.selected_fill),
        _ => None,
    };
    container::Style {
        background: fill.map(Background::Color),
        border: Border::default().rounded(4.0),
        text_color: Some(if selected {
            tokens.marked
        } else if cursor {
            tokens.cursor_text
        } else {
            tokens.list_text
        }),
        ..container::Style::default()
    }
}

/// A round badge `size` wide with a number in it - a login script's step (RC-050-13): the
/// header's fill in a hairline.
pub fn badge(tokens: &Tokens, size: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.header)),
        border: Border::default()
            .rounded(size / 2.0)
            .width(1.0)
            .color(tokens.stroke),
        text_color: Some(tokens.text),
        ..container::Style::default()
    }
}

/// The small accent pill on the left of the active panel's cursor row (WinUI's list selection).
pub fn pill(tokens: &Tokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.accent)),
        border: Border::default().rounded(1.5),
        ..container::Style::default()
    }
}

/// The file list's scroll bar thumb (RC-010-23): `text_secondary` at half strength, stronger
/// under the pointer and strongest while held, as a Windows scroll bar's.
pub fn thumb_fill(tokens: &Tokens, hovered: bool, pressed: bool) -> Color {
    let alpha = if pressed {
        1.0
    } else if hovered {
        0.75
    } else {
        0.5
    };
    tokens.text_secondary.scale_alpha(alpha)
}

/// A function-key button of the bottom bar (inert in the spike).
pub fn key_button(tokens: &Tokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.control)),
        border: Border::default()
            .rounded(4.0)
            .width(1.0)
            .color(tokens.stroke),
        text_color: Some(tokens.text),
        ..container::Style::default()
    }
}

/// The dimmed window behind a dialog.
pub fn backdrop(_tokens: &Tokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(hex(0x000000, 0.35))),
        ..container::Style::default()
    }
}

/// A dialog's card: the surface with a border and a shadow, like a WinUI content dialog.
pub fn dialog(tokens: &Tokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.surface)),
        border: Border::default()
            .rounded(8.0)
            .width(1.0)
            .color(tokens.stroke),
        shadow: Shadow {
            color: hex(0x000000, if tokens.dark { 0.5 } else { 0.2 }),
            offset: iced::Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        text_color: Some(tokens.text),
        snap: true,
    }
}

/// The id of a dialog card whose focus ring Tab and Shift+Tab walk (`app/field_walk.rs`,
/// `theme::focus`).
pub const DIALOG_ID: &str = "rcommander.dialog";

/// The space a large dialog keeps from the window's edges.
pub const LARGE_DIALOG_MARGIN: f32 = 24.0;

/// The rows of [`ROW_HEIGHT`] that fit a list `height` high; one at least.
pub fn rows_for(height: f32) -> usize {
    (height / ROW_HEIGHT).floor().max(1.0) as usize
}

/// An entry of a dialog's page list: the chosen one filled and in the accent colour.
pub fn page_entry(tokens: &Tokens, chosen: bool, status: button::Status) -> button::Style {
    let fill = match (chosen, status) {
        (true, _) => Some(tokens.cursor_fill),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            Some(tokens.cursor_fill_inactive)
        }
        _ => None,
    };
    button::Style {
        background: fill.map(Background::Color),
        text_color: if chosen { tokens.accent } else { tokens.text },
        border: Border::default().rounded(4.0),
        ..button::Style::default()
    }
}

/// The terminal's colours: Windows Terminal's Campbell on dark, One Half Light on light.
pub fn terminal_palette(tokens: &Tokens) -> rc_term_widget::ColorPalette {
    let [
        fg,
        bg,
        black,
        red,
        green,
        yellow,
        blue,
        magenta,
        cyan,
        white,
    ] = if tokens.dark {
        [
            "#cccccc", "#1c1c1c", "#0c0c0c", "#c50f1f", "#13a10e", "#c19c00", "#0037da", "#881798",
            "#3a96dd", "#cccccc",
        ]
    } else {
        [
            "#383a42", "#ffffff", "#383a42", "#e45649", "#50a14f", "#c18401", "#0184bc", "#a626a4",
            "#0997b3", "#fafafa",
        ]
    };
    let [
        b_black,
        b_red,
        b_green,
        b_yellow,
        b_blue,
        b_magenta,
        b_cyan,
        b_white,
    ] = if tokens.dark {
        [
            "#767676", "#e74856", "#16c60c", "#f9f1a5", "#3b78ff", "#b4009e", "#61d6d6", "#f2f2f2",
        ]
    } else {
        [
            "#4f525d", "#df6c75", "#98c379", "#e4c07a", "#61afef", "#c577dd", "#56b5c1", "#ffffff",
        ]
    };
    let s = str::to_owned;
    rc_term_widget::ColorPalette {
        foreground: s(fg),
        background: s(bg),
        black: s(black),
        red: s(red),
        green: s(green),
        yellow: s(yellow),
        blue: s(blue),
        magenta: s(magenta),
        cyan: s(cyan),
        white: s(white),
        bright_black: s(b_black),
        bright_red: s(b_red),
        bright_green: s(b_green),
        bright_yellow: s(b_yellow),
        bright_blue: s(b_blue),
        bright_magenta: s(b_magenta),
        bright_cyan: s(b_cyan),
        bright_white: s(b_white),
        bright_foreground: None,
        dim_foreground: s(b_black),
        dim_black: s(black),
        dim_red: s(red),
        dim_green: s(green),
        dim_yellow: s(yellow),
        dim_blue: s(blue),
        dim_magenta: s(magenta),
        dim_cyan: s(cyan),
        dim_white: s(white),
    }
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
