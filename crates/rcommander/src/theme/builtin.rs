//! The built-in themes' colours (RC-100-50): Fluent (WinUI's light and dark sets, `light.rs`,
//! `dark.rs`), Mac (macOS only, `mac.rs`, RC-100-49), Classic (Total Commander's white list with a
//! blue cursor, and Norton Commander's blue panels for the dark mode), High contrast, Solarized
//! and Nord - each light and dark, each with its terminal colours. The engine names them
//! (`rc_engine::settings::themes::BUILTIN_THEMES`); `builtin_tests.rs` holds the two lists and
//! every look to WCAG AA's 4.5:1 for its text.
//!
//! Solarized (Ethan Schoonover, <https://ethanschoonover.com/solarized/>) and Nord (Arctic Ice
//! Studio, <https://www.nordtheme.com/>) are MIT-licensed palettes, credited in `NOTICE`; their
//! terminal colours are the palettes' own, their syntax colours lightened (on dark) or darkened
//! (on light) where the original falls below 4.5:1 on the theme's surface.

mod classic;
mod high_contrast;
mod nord;
mod solarized;

use iced::Color;

use super::{Syntax, Tokens, dark, hex, light, terminal::Terminal};
use crate::i18n::t;

/// A built-in look's colours, every one opaque `0xRRGGBB`.
pub(super) struct Palette {
    pub dark: bool,
    pub window: u32,
    pub surface: u32,
    pub header: u32,
    pub stroke: u32,
    pub text: u32,
    pub text_secondary: u32,
    pub accent: u32,
    pub cursor: u32,
    pub cursor_inactive: u32,
    pub selected: u32,
    pub cursor_text: u32,
    pub marked: u32,
    pub list_fill: u32,
    pub list_text: u32,
    pub list_text_secondary: u32,
    pub stripe: u32,
    pub control: u32,
    /// Keyword, key, string, number, literal, comment, punctuation, operator, type, function,
    /// invalid.
    pub syntax: [u32; 11],
    /// Foreground, background, the eight normal and the eight bright ANSI colours.
    pub terminal: [u32; 18],
}

impl Palette {
    /// The look of these colours.
    fn tokens(&self) -> Tokens {
        let c = |rgb: u32| hex(rgb, 1.0);
        Tokens {
            dark: self.dark,
            window: c(self.window),
            surface: c(self.surface),
            header: c(self.header),
            stroke: c(self.stroke),
            text: c(self.text),
            text_secondary: c(self.text_secondary),
            accent: c(self.accent),
            cursor_fill: c(self.cursor),
            cursor_fill_inactive: c(self.cursor_inactive),
            selected_fill: c(self.selected),
            stripe_fill: c(self.stripe),
            control: c(self.control),
            list_fill: c(self.list_fill),
            list_text: c(self.list_text),
            list_text_secondary: c(self.list_text_secondary),
            cursor_text: c(self.cursor_text),
            marked: c(self.marked),
            syntax: Syntax::of(self.syntax),
            terminal: Terminal::of(self.terminal),
        }
    }
}

/// The look of the built-in theme `id` with `accent`, the system's accent for its mode, where the
/// theme follows it; `None` for an id this build or this system does not have.
pub fn look(id: &str, accent: Color) -> Option<Tokens> {
    Some(match id {
        "fluent-light" => light::tokens(accent),
        "fluent-dark" => dark::tokens(accent),
        #[cfg(target_os = "macos")]
        "mac-light" => super::mac::light(accent),
        #[cfg(target_os = "macos")]
        "mac-dark" => super::mac::dark(accent),
        "classic-light" => classic::LIGHT.tokens(),
        "classic-dark" => classic::DARK.tokens(),
        "high-contrast-light" => high_contrast::LIGHT.tokens(),
        "high-contrast-dark" => high_contrast::DARK.tokens(),
        "solarized-light" => solarized::LIGHT.tokens(),
        "solarized-dark" => solarized::DARK.tokens(),
        "nord-light" => nord::LIGHT.tokens(),
        "nord-dark" => nord::DARK.tokens(),
        _ => return None,
    })
}

/// Whether the built-in theme `id` draws with the system's accent (Fluent, Mac); the others
/// bring their own.
pub fn follows_system_accent(id: &str) -> bool {
    matches!(
        id,
        "fluent-light" | "fluent-dark" | "mac-light" | "mac-dark"
    )
}

/// The name of the built-in theme `id` in the interface's language; `None` for an own theme.
pub fn name(id: &str) -> Option<&'static str> {
    Some(match id {
        "fluent-light" | "fluent-dark" => t!("theme.builtin.fluent"),
        "mac-light" | "mac-dark" => t!("theme.builtin.mac"),
        "classic-light" | "classic-dark" => t!("theme.builtin.classic"),
        "high-contrast-light" | "high-contrast-dark" => t!("theme.builtin.high_contrast"),
        "solarized-light" | "solarized-dark" => t!("theme.builtin.solarized"),
        "nord-light" | "nord-dark" => t!("theme.builtin.nord"),
        _ => return None,
    })
}

#[cfg(test)]
#[path = "builtin_tests.rs"]
mod tests;
