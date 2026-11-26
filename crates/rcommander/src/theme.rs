//! The Fluent-like look: Windows 11's colour tokens for light and dark, its fonts, and the style
//! functions the views use.
//!
//! The colours follow WinUI's theme resources (`SolidBackgroundFillColorBase`, the card and
//! stroke fills, `TextFillColorPrimary/Secondary`, the default accent). iced reports the system's
//! light or dark mode ([`iced::system::theme`]); the user's accent colour comes from the registry
//! on Windows (`system.rs`), Windows' default blue elsewhere. The setting `appearance.theme`
//! chooses light, dark or the system's look ([`ThemeChoice`]). The colours of each mode are its
//! colour theme's (RC-100-50, `themes.rs`): a built-in one (`builtin.rs`) or the person's own,
//! the terminal's colours with them (`terminal.rs`); Fluent (the sets of `light.rs` and `dark.rs`)
//! until the themes are read. On macOS `appearance.macos_look` (RC-100-49, on) draws with the
//! Mac's accent and fonts ([`mac_look`]) and starts with the Mac theme. While Windows' high
//! contrast is on, every look is the High contrast theme in the system's colours, and Windows'
//! text size scales the interface (`access.rs`, RC-110-79).

pub mod access;
pub mod builtin;
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
pub mod status;
pub mod symbols;
mod syntax;
mod system;
mod system_fonts;
pub mod terminal;
pub mod themes;
pub mod wheel;

use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub use chrome::{
    alternate, drive_line, drive_line_rule, header_cell, set_stripes, striped_row, tab,
};
pub use layout::large_dialog;
pub use syntax::Syntax;
pub use terminal::terminal_palette;

use iced::{
    Background, Border, Color, Font, Shadow, Theme,
    font::Weight,
    theme::{Palette, palette::Extended},
    widget::{button, container, progress_bar, slider, text_input},
};

/// The colours of one appearance.
#[derive(Debug, Clone)]
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
    /// The terminal's colours (RC-100-50).
    pub terminal: terminal::Terminal,
    /// Danger, warning, success and the fields' border, fitted to this look's backgrounds
    /// (RC-110-77, `status.rs`).
    pub status: status::Status,
}

const fn hex(rgb: u32, alpha: f32) -> Color {
    Color {
        r: ((rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: (rgb & 0xff) as f32 / 255.0,
        a: alpha,
    }
}

/// Fluent's light and dark looks with the system's accent: the looks before the themes are read.
static FIXED: OnceLock<[Tokens; 2]> = OnceLock::new();
/// `appearance.macos_look`, as the start read it: the tokens are made once.
static MAC_LOOK: AtomicBool = AtomicBool::new(true);
/// The system's accent for the light and the dark look, read once.
static ACCENT: OnceLock<system::Accent> = OnceLock::new();

/// The system's accent for the light and the dark look, as [`tokens`] draws with it unless the
/// theme brings its own (RC-100-50).
pub fn system_accent() -> [Color; 2] {
    let accent = ACCENT.get_or_init(system::accent);
    [accent.light, accent.dark]
}

/// Takes the setting `appearance.macos_look` for this run, sets the system's font families on
/// Linux (`system_fonts.rs`) and reads the system's accent colour and its accessibility settings
/// (`access.rs`); the start calls it before the window opens, so drawing never touches the
/// registry and the first window opens at the system's text size. Later calls change nothing but
/// the accessibility settings, which `access::changes` follows from then on.
pub fn init(mac_look: bool) {
    MAC_LOOK.store(mac_look, Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    if mac_look {
        mac::fonts();
    }
    // The system's families instead of cosmic-text's Open Sans (RC-100-71).
    #[cfg(all(unix, not(target_os = "macos")))]
    system_fonts::apply();
    tokens(false);
    access::apply(access::read());
}

/// Whether this run draws the Mac's look: on macOS while `appearance.macos_look` is on; the
/// colours, the accent, the fonts and the title bar follow it.
pub fn mac_look() -> bool {
    cfg!(target_os = "macos") && MAC_LOOK.load(Ordering::Relaxed)
}

/// The tokens of the light or the dark look: its colour theme's ([`themes::current`]), the
/// editor's preview while it edits one of that mode, Fluent's before the themes are read; while
/// Windows' high contrast is on, the High contrast theme in its colours for both
/// ([`access::contrast_look`], RC-110-79).
pub fn tokens(dark: bool) -> &'static Tokens {
    if let Some(contrast) = access::contrast_look() {
        return contrast;
    }
    themes::current(dark).unwrap_or_else(|| fixed(dark))
}

/// Fluent's look of the light or the dark mode with the system's accent.
pub(crate) fn fixed(dark: bool) -> &'static Tokens {
    let [light, dark_tokens] = FIXED.get_or_init(|| {
        let [light, dark] = system_accent();
        [light::tokens(light), dark::tokens(dark)]
    });
    if dark { dark_tokens } else { light }
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

/// The iced theme the built-in widgets draw with, from the same tokens: the look's status
/// colours (`status.rs`), and its field border as the strong background shade, the border iced
/// draws around every text field, text editor, pick list and check box without focus - 3:1
/// against the window and the card where iced's own fell to 1.6:1 (RC-110-77).
pub fn iced_theme(tokens: &Tokens) -> Theme {
    let status = tokens.status;
    let palette = Palette {
        background: tokens.window,
        text: tokens.text,
        primary: tokens.accent,
        success: status.success,
        warning: status.warning,
        danger: status.danger,
    };
    let name = if tokens.dark {
        "Fluent Dark"
    } else {
        "Fluent Light"
    };
    Theme::custom_with_fn(name, palette, move |palette| {
        let mut extended = Extended::generate(palette);
        extended.background.strong.color = status.field;
        extended
    })
}

/// iced's own strong background shade of `theme`, before the field border took it
/// ([`iced_theme`]): the track of a progress bar and the empty part of a slider's rail, where the
/// field border would stand too close to the dark look's light accent (RC-110-77).
fn track(theme: &Theme) -> Background {
    Background::Color(Extended::generate(theme.palette()).background.strong.color)
}

/// A progress bar: the accent on iced's own track.
pub fn progress(theme: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: track(theme),
        ..progress_bar::primary(theme)
    }
}

/// A slider (the viewer's scroll bar): iced's own, the empty part of its rail on iced's own
/// track.
pub fn scroll_slider(theme: &Theme, status: slider::Status) -> slider::Style {
    let mut style = slider::default(theme, status);
    style.rail.backgrounds.1 = track(theme);
    style
}

/// A text field: iced's own, its placeholder moved towards the text until it reads at 4.5:1 on
/// the field - iced's own fell to 2.95:1 (RC-110-78). A placeholder is an example or a hint; the
/// field's name stands in its label beside or over it (`layout::captioned`), which stays while
/// the field is typed in. A disabled field keeps iced's look.
pub fn text_field(theme: &Theme, state: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, state);
    if state != text_input::Status::Disabled {
        let palette = theme.extended_palette();
        style.placeholder = status::readable(
            style.placeholder,
            palette.is_dark,
            &[palette.background.base.color],
            colours::MIN_CONTRAST,
        );
    }
    style
}

/// The interface font: Segoe UI Variable on Windows 11, the system's sans-serif elsewhere (and
/// on Windows 10, where the name does not resolve and cosmic-text falls back to Segoe UI) - San
/// Francisco on macOS with the Mac's look ([`mac_look`]), fontconfig's choice on Linux
/// (`system_fonts.rs`).
pub const UI_FONT: Font = if cfg!(windows) {
    Font::with_name("Segoe UI Variable")
} else {
    Font::DEFAULT
};

/// The interface font, semibold.
pub const UI_FONT_STRONG: Font = Font {
    weight: Weight::Semibold,
    ..UI_FONT
};

/// The monospace font: Cascadia Mono (Windows 11, Windows Terminal) where present, Menlo on
/// macOS with the Mac's look; the terminal's and the viewer's until another is chosen
/// ([`fonts`]).
pub const MONO_FONT: Font = if cfg!(windows) {
    Font::with_name("Cascadia Mono")
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
/// The inactive panel's cursor carries a 1-px outline in the lists' quieter text as well: its
/// fill alone is 1.1:1 on the list (RC-110-77).
pub fn row(tokens: &Tokens, cursor: bool, active: bool, selected: bool) -> container::Style {
    let fill = match (cursor, active, selected) {
        (true, true, _) => Some(tokens.cursor_fill),
        (true, false, _) => Some(tokens.cursor_fill_inactive),
        (false, _, true) => Some(tokens.selected_fill),
        _ => None,
    };
    let border = Border::default().rounded(4.0);
    container::Style {
        background: fill.map(Background::Color),
        border: if cursor && !active {
            border.width(1.0).color(tokens.list_text_secondary)
        } else {
            border
        },
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

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
