//! The look and its colour themes (RC-100-50): `appearance.theme` - the system's, light or dark -
//! decides which mode is in force; `appearance.theme_light` and `appearance.theme_dark` name the
//! theme of each, a built-in or an own one's id (`settings/themes.rs`). A setting that names no
//! theme of its mode draws with Fluent, with a warning. None is protected: a theme changes what
//! the person sees, never what is worked on.

use super::{SettingKind, SettingPage, SettingSpec, choices};

/// The light and dark look: the system's, light or dark.
pub const THEME_KEY: &str = "appearance.theme";
/// The theme of the light mode.
pub const THEME_LIGHT_KEY: &str = "appearance.theme_light";
/// The theme of the dark mode.
pub const THEME_DARK_KEY: &str = "appearance.theme_dark";
/// Whether a connection's panel and SSH terminal draw with the connection's own theme
/// (RC-100-51); off, every panel draws with the window's, the themes stay assigned.
pub const CONNECTION_THEMES_KEY: &str = "appearance.connection_themes";

pub(super) const THEME: SettingSpec = SettingSpec {
    key: THEME_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Choice {
        choices: choices::THEMES,
        default: "system",
    },
    label: "setting.appearance.theme",
    help: "setting.appearance.theme.help",
    protected: false,
    restart: false,
};

pub(super) const THEME_LIGHT: SettingSpec = SettingSpec {
    key: THEME_LIGHT_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Theme {
        default: "fluent-light",
    },
    label: "setting.appearance.theme_light",
    help: "setting.appearance.theme_light.help",
    protected: false,
    restart: false,
};

pub(super) const THEME_DARK: SettingSpec = SettingSpec {
    key: THEME_DARK_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Theme {
        default: "fluent-dark",
    },
    label: "setting.appearance.theme_dark",
    help: "setting.appearance.theme_dark.help",
    protected: false,
    restart: false,
};

pub(super) const CONNECTION_THEMES: SettingSpec = SettingSpec {
    key: CONNECTION_THEMES_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.appearance.connection_themes",
    help: "setting.appearance.connection_themes.help",
    protected: false,
    restart: false,
};
