//! The colour themes of the WSL distributions (RC-100-56): `appearance.wsl_themes` names, per
//! distribution, a light and a dark theme a panel on its share and its WSL terminal draw with -
//! one line each, `Distro<TAB>light-id<TAB>dark-id`, either id empty for the window's. Stored as
//! one setting, so it needs no table and goes with the export of the settings (RC-080-19).
//!
//! The text is checked here for its shape - a distribution's name as WSL's share spells it, a
//! theme id's characters, at most [`MAX_WSL_THEMES`] lines; which themes there are and their mode
//! are checked on the way in (`commands/connections_themes.rs`), since only the database knows
//! the own ones. Names compare without regard to case; a later line for a name replaces an
//! earlier one, and a line without either theme says nothing and is dropped.

use super::{SettingKind, SettingPage, SettingSpec};

/// The themes of the WSL distributions.
pub const WSL_THEMES_KEY: &str = "appearance.wsl_themes";
/// The most distributions the setting names.
pub const MAX_WSL_THEMES: usize = 64;
/// The longest distribution name the setting takes; real ones are far shorter.
const MAX_DISTRO: usize = 64;
/// The longest theme id, as `appearance.theme_light` takes it.
const MAX_THEME_ID: usize = 64;

pub(super) const WSL_THEMES: SettingSpec = SettingSpec {
    key: WSL_THEMES_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::WslThemes { default: "" },
    label: "setting.appearance.wsl_themes",
    help: "setting.appearance.wsl_themes.help",
    protected: false,
    restart: false,
};

/// One distribution's themes: the light and the dark theme's id, `None` for the window's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslTheme {
    /// The distribution, as its share spells it (`Ubuntu`, `Debian`).
    pub distro: String,
    pub light: Option<String>,
    pub dark: Option<String>,
}

impl WslTheme {
    /// The theme of the light or the `dark` mode.
    #[must_use]
    pub fn theme(&self, dark: bool) -> Option<&str> {
        if dark {
            self.dark.as_deref()
        } else {
            self.light.as_deref()
        }
    }
}

/// Whether `name` can be a distribution's name in the setting: what WSL's share takes as one
/// (`rc_vfs::parse_wsl_unc`), no tab, no space around it.
fn distro_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_DISTRO
        && name.trim() == name
        && name != "."
        && name != ".."
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

/// A theme id field: `Some(None)` when empty, `Some(Some(id))` for an id's shape (lower-case
/// letters, digits and `-`), `None` otherwise.
fn theme_id(text: &str) -> Option<Option<String>> {
    let text = text.trim();
    if text.is_empty() {
        return Some(None);
    }
    let fits = text.len() <= MAX_THEME_ID
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    fits.then(|| Some(text.to_owned()))
}

/// The distributions' themes in `text`; `None` when a line is no `Distro<TAB>light[<TAB>dark]`
/// or there are more than [`MAX_WSL_THEMES`].
#[must_use]
pub fn parse_wsl_themes(text: &str) -> Option<Vec<WslTheme>> {
    let mut rows: Vec<WslTheme> = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split('\t');
        let distro = fields.next()?.trim();
        let light = theme_id(fields.next()?)?;
        let dark = match fields.next() {
            Some(dark) => theme_id(dark)?,
            None => None,
        };
        if !distro_name(distro) || fields.next().is_some() {
            return None;
        }
        rows.retain(|row| !row.distro.eq_ignore_ascii_case(distro));
        if light.is_some() || dark.is_some() {
            rows.push(WslTheme {
                distro: distro.to_owned(),
                light,
                dark,
            });
        }
        if rows.len() > MAX_WSL_THEMES {
            return None;
        }
    }
    Some(rows)
}

/// `rows` written as the setting keeps them: one line each, both ids, empty for none.
#[must_use]
pub fn write_wsl_themes(rows: &[WslTheme]) -> String {
    rows.iter()
        .filter(|row| row.light.is_some() || row.dark.is_some())
        .map(|row| {
            let light = row.light.as_deref().unwrap_or_default();
            let dark = row.dark.as_deref().unwrap_or_default();
            format!("{}\t{light}\t{dark}", row.distro)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The themes of `distro` among `rows`, its name compared without regard to case.
#[must_use]
pub fn wsl_theme<'a>(rows: &'a [WslTheme], distro: &str) -> Option<&'a WslTheme> {
    rows.iter()
        .find(|row| row.distro.eq_ignore_ascii_case(distro))
}

#[cfg(test)]
#[path = "wsl_themes_tests.rs"]
mod tests;
