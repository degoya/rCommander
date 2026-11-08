//! Where a theme is used (RC-110-09): the environments the Themes tab's pick lists assign - the
//! window's light and dark mode, and where the tab shows them (`wsl::SHOWN`) each WSL
//! distribution's light or dark mode (RC-100-56) - as the draft has them, and what using a theme
//! for one of them edits. A theme fits its own mode only, as the pick lists offer it. Pure, tested
//! without a window; the row's tags and the row's context menu (`menu.rs`) read it.

use rc_engine::settings::{
    THEME_DARK_KEY, THEME_LIGHT_KEY, WSL_THEMES_KEY,
    themes::{Theme, ThemesState},
    wsl_theme,
};

use super::wsl;
use crate::{
    app::settings::draft::{Draft, Field},
    i18n::t,
};

/// An environment a theme is assigned to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    /// The window's light or dark mode.
    Mode { dark: bool },
    /// A WSL distribution's panels and terminal in the light or the dark mode.
    Distro { name: String, dark: bool },
}

impl Environment {
    /// The tag of the environment in a theme's row.
    pub fn tag(&self) -> String {
        match self {
            Self::Mode { dark: false } => t!("settings.themes.used_light").to_owned(),
            Self::Mode { dark: true } => t!("settings.themes.used_dark").to_owned(),
            Self::Distro { name, .. } => t!("settings.themes.used_wsl", distro = name.as_str()),
        }
    }

    /// The label of the context menu's entry that uses a theme for the environment.
    pub fn use_label(&self) -> String {
        match self {
            Self::Mode { dark: false } => t!("settings.themes.menu.use_light").to_owned(),
            Self::Mode { dark: true } => t!("settings.themes.menu.use_dark").to_owned(),
            Self::Distro { name, .. } => {
                t!("settings.themes.menu.use_wsl", distro = name.as_str())
            }
        }
    }
}

/// The theme the draft names for the light or the `dark` mode; the one in force where it names
/// none (or one that is gone).
pub(super) fn mode_theme<'a>(draft: &'a Draft, state: &'a ThemesState, dark: bool) -> &'a str {
    let key = if dark {
        THEME_DARK_KEY
    } else {
        THEME_LIGHT_KEY
    };
    match draft.field(key) {
        Some(Field::Text(text)) if state.theme(text).is_some() => text.as_str(),
        _ => state.chosen[usize::from(dark)].as_str(),
    }
}

/// The environments `theme` can be used for: its own mode, then each distribution of `distros`
/// in that mode (`distros` empty where the tab shows no WSL section).
pub(super) fn offered(theme: &Theme, distros: &[String]) -> Vec<Environment> {
    let dark = theme.dark;
    std::iter::once(Environment::Mode { dark })
        .chain(distros.iter().map(|name| Environment::Distro {
            name: name.clone(),
            dark,
        }))
        .collect()
}

/// Whether the draft uses the theme `id` for `environment`.
pub(super) fn uses(
    environment: &Environment,
    id: &str,
    draft: &Draft,
    state: &ThemesState,
) -> bool {
    match environment {
        Environment::Mode { dark } => mode_theme(draft, state, *dark) == id,
        Environment::Distro { name, dark } => {
            let rows = wsl::drafted_rows(draft, state);
            wsl_theme(&rows, name).and_then(|row| row.theme(*dark)) == Some(id)
        }
    }
}

/// The environments that use `theme` as the draft stands: its mode where the mode draws with it,
/// then - with `wsl` - each distribution whose line names it, also one the drive bar does not
/// find now.
pub(super) fn used_by(
    theme: &Theme,
    draft: &Draft,
    state: &ThemesState,
    wsl: bool,
) -> Vec<Environment> {
    let dark = theme.dark;
    let mut used = Vec::new();
    if mode_theme(draft, state, dark) == theme.id {
        used.push(Environment::Mode { dark });
    }
    if wsl {
        let rows = wsl::drafted_rows(draft, state);
        used.extend(
            rows.iter()
                .filter(|row| row.theme(dark) == Some(theme.id.as_str()))
                .map(|row| Environment::Distro {
                    name: row.distro.clone(),
                    dark,
                }),
        );
    }
    used
}

/// What using the theme `id` for `environment` edits: the setting's key and its new text, as the
/// environment's pick list writes it.
pub(super) fn assign(
    environment: &Environment,
    id: &str,
    draft: &Draft,
    state: &ThemesState,
) -> (&'static str, String) {
    match environment {
        Environment::Mode { dark: false } => (THEME_LIGHT_KEY, id.to_owned()),
        Environment::Mode { dark: true } => (THEME_DARK_KEY, id.to_owned()),
        Environment::Distro { name, dark } => {
            let rows = wsl::drafted_rows(draft, state);
            let text = wsl::with_theme(rows, name, *dark, Some(id.to_owned()));
            (WSL_THEMES_KEY, text)
        }
    }
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
