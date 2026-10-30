//! The language of the interface: the one the setting names, else the desktop's when rCommander
//! ships it, English otherwise.
//!
//! The setting `appearance.language` (RC-100-47) is read once at the start
//! ([`choose_setting`]); `auto` leaves the choice to the desktop. The desktop's language is read
//! once, after the pattern of rDownloader's `rd-capture/src/locale.rs`: the POSIX variables
//! first - a Linux or macOS session's language, and the way to choose one on any system - then,
//! only when none of them names a language of ours, the system's preferred languages (`sys-locale`:
//! the display languages on Windows, the preferences on macOS). A language is a variant here and
//! a file in `locales/`.

use std::sync::OnceLock;

/// The languages the interface is written in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lang {
    En,
    De,
    Fr,
    Es,
}

impl Lang {
    /// The language of a two-letter code, in any case.
    fn of_code(code: &str) -> Option<Self> {
        [
            ("en", Self::En),
            ("de", Self::De),
            ("fr", Self::Fr),
            ("es", Self::Es),
        ]
        .into_iter()
        .find(|(name, _)| code.eq_ignore_ascii_case(name))
        .map(|(_, lang)| lang)
    }

    /// The language a locale tag names - `de_DE.UTF-8`, `fr-CH`, `es` - when it is one of ours;
    /// `None` for any other and for `C` or `POSIX`.
    pub fn of_tag(tag: &str) -> Option<Self> {
        let primary = tag.trim().split(['_', '-', '.', '@']).next()?;
        Self::of_code(primary)
    }

    /// The first of `tags` that names one of ours.
    pub fn find<I, S>(tags: I) -> Option<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        tags.into_iter().find_map(|tag| Self::of_tag(tag.as_ref()))
    }

    /// The language the setting `appearance.language` names; `None` for `auto` and for a value
    /// that is no choice of the row.
    pub fn of_setting(value: &str) -> Option<Self> {
        Self::of_code(value)
    }
}

/// The language the setting chose, set once at the start; unset, the desktop's counts.
static CHOSEN: OnceLock<Lang> = OnceLock::new();

/// Takes the stored setting `value` as the language of this run. Called once at the start,
/// before the window builds its first text; a change takes effect at the next start.
pub fn choose_setting(value: &str) {
    if let Some(lang) = Lang::of_setting(value) {
        let _ = CHOSEN.set(lang);
    }
}

/// The language of the interface: the setting's, else the desktop's.
pub fn current() -> Lang {
    CHOSEN.get().copied().unwrap_or_else(desktop)
}

/// The desktop's language, read on first use. The tests read English whatever the machine's
/// language: the texts they expect are English.
fn desktop() -> Lang {
    if cfg!(test) {
        return Lang::En;
    }
    static DESKTOP: OnceLock<Lang> = OnceLock::new();
    *DESKTOP.get_or_init(|| choose(&variable_tags(|name| std::env::var(name).ok()), system_tags))
}

/// The variables before the system: `system` is asked only when no variable names one of ours,
/// and English is the answer when neither does.
fn choose(variables: &[String], system: impl FnOnce() -> Vec<String>) -> Lang {
    Lang::find(variables)
        .or_else(|| Lang::find(system()))
        .unwrap_or(Lang::En)
}

/// The locale tags the POSIX variables state, in the order gettext reads them; `LANGUAGE` may
/// list several, separated by colons.
fn variable_tags(lookup: impl Fn(&str) -> Option<String>) -> Vec<String> {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(lookup)
        .flat_map(|value| value.split(':').map(str::to_owned).collect::<Vec<_>>())
        .collect()
}

/// The languages the user prefers, as the system lists them.
fn system_tags() -> Vec<String> {
    sys_locale::get_locales().collect()
}

#[cfg(test)]
#[path = "locale_tests.rs"]
mod tests;
