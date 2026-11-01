//! The built-in themes (RC-100-50, RC-100-65): their ids, English names and modes, in the order
//! the window lists them; their colours are the window's (`crates/rcommander/src/theme/builtin/`).

use super::{DEFAULT_DARK, DEFAULT_LIGHT};

/// A built-in theme: its id, its English name (the window shows it translated) and its mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinTheme {
    pub id: &'static str,
    pub name: &'static str,
    pub dark: bool,
    /// Offered on macOS only: the Mac's own greys (RC-100-49).
    pub macos_only: bool,
}

const fn builtin_row(id: &'static str, name: &'static str, dark: bool) -> BuiltinTheme {
    BuiltinTheme {
        id,
        name,
        dark,
        macos_only: false,
    }
}

/// The built-in themes, light and dark of each, in the order the window lists them.
pub const BUILTIN_THEMES: [BuiltinTheme; 28] = [
    builtin_row(DEFAULT_LIGHT, "Fluent", false),
    builtin_row(DEFAULT_DARK, "Fluent", true),
    BuiltinTheme {
        macos_only: true,
        ..builtin_row("mac-light", "Mac", false)
    },
    BuiltinTheme {
        macos_only: true,
        ..builtin_row("mac-dark", "Mac", true)
    },
    builtin_row("classic-light", "Classic", false),
    builtin_row("classic-dark", "Classic", true),
    builtin_row("high-contrast-light", "High contrast", false),
    builtin_row("high-contrast-dark", "High contrast", true),
    builtin_row("solarized-light", "Solarized", false),
    builtin_row("solarized-dark", "Solarized", true),
    builtin_row("nord-light", "Nord", false),
    builtin_row("nord-dark", "Nord", true),
    builtin_row("gruvbox-light", "Gruvbox", false),
    builtin_row("gruvbox-dark", "Gruvbox", true),
    builtin_row("dracula-light", "Dracula", false),
    builtin_row("dracula-dark", "Dracula", true),
    builtin_row("catppuccin-light", "Catppuccin", false),
    builtin_row("catppuccin-dark", "Catppuccin", true),
    builtin_row("tokyo-night-light", "Tokyo Night", false),
    builtin_row("tokyo-night-dark", "Tokyo Night", true),
    builtin_row("everforest-light", "Everforest", false),
    builtin_row("everforest-dark", "Everforest", true),
    builtin_row("rose-pine-light", "Ros\u{e9} Pine", false),
    builtin_row("rose-pine-dark", "Ros\u{e9} Pine", true),
    builtin_row("kanagawa-light", "Kanagawa", false),
    builtin_row("kanagawa-dark", "Kanagawa", true),
    builtin_row("ayu-light", "Ayu", false),
    builtin_row("ayu-dark", "Ayu", true),
];

impl BuiltinTheme {
    /// Whether this system offers it.
    #[must_use]
    pub fn available(&self) -> bool {
        !self.macos_only || cfg!(target_os = "macos")
    }
}

/// The built-in theme `id` this system offers.
#[must_use]
pub fn builtin(id: &str) -> Option<&'static BuiltinTheme> {
    BUILTIN_THEMES
        .iter()
        .find(|theme| theme.id == id && theme.available())
}
