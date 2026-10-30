//! The page Appearance: the language of the interface (RC-100-47, owner 2026-10-29).
//!
//! Automatic follows the system's language when rCommander ships it, English otherwise; the
//! other choices name one of the four catalogues. It takes effect at the next start: the window
//! keeps texts it built once (the native menus, the tabs' and dialogs' titles), so switching
//! while it runs would leave them in the old language. Not protected: it changes only the words.
//! Each language's name is written in that language, in every catalogue.

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// The language of the interface: `auto` or a language code.
pub const LANGUAGE_KEY: &str = "appearance.language";

/// The choices, Automatic first; the values are the codes `locale::Lang::of_setting` reads.
const LANGUAGES: &[Choice] = &[
    Choice {
        value: "auto",
        label: "setting.appearance.language.auto",
    },
    Choice {
        value: "en",
        label: "setting.appearance.language.en",
    },
    Choice {
        value: "de",
        label: "setting.appearance.language.de",
    },
    Choice {
        value: "fr",
        label: "setting.appearance.language.fr",
    },
    Choice {
        value: "es",
        label: "setting.appearance.language.es",
    },
];

pub(super) const LANGUAGE: SettingSpec = SettingSpec {
    key: LANGUAGE_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Choice {
        choices: LANGUAGES,
        default: "auto",
    },
    label: "setting.appearance.language",
    help: "setting.appearance.language.help",
    protected: false,
    restart: true,
};
