//! The language of the interface: the desktop's, when rCommander ships it, English otherwise.
//!
//! Read once, after the pattern of rDownloader's `rd-capture/src/locale.rs`: the POSIX variables
//! first - a Linux or macOS session's language, and the way to choose one on any system - then,
//! only when none of them names a language of ours, the system's preferred languages
//! (`sys-locale`: the display languages on Windows, the preferences on macOS). English is the only
//! catalogue until after 1.0, so every answer is English for now; a second language is a variant
//! here and a file in `locales/`.

use std::sync::OnceLock;

/// The languages the interface is written in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lang {
    En,
}

impl Lang {
    /// The language a locale tag names - `en_GB.UTF-8`, `en-US`, `en` - when it is one of ours;
    /// `None` for any other and for `C` or `POSIX`.
    pub fn of_tag(tag: &str) -> Option<Self> {
        let primary = tag.trim().split(['_', '-', '.', '@']).next()?;
        primary.eq_ignore_ascii_case("en").then_some(Self::En)
    }

    /// The first of `tags` that names one of ours.
    pub fn find<I, S>(tags: I) -> Option<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        tags.into_iter().find_map(|tag| Self::of_tag(tag.as_ref()))
    }
}

/// The desktop's language, read on first use.
pub fn current() -> Lang {
    static CURRENT: OnceLock<Lang> = OnceLock::new();
    *CURRENT.get_or_init(|| choose(&variable_tags(|name| std::env::var(name).ok()), system_tags))
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
