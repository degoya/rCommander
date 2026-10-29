//! The choices of the rows `schema.rs` writes out itself: the look, the file icons, the quick
//! search's keys. Split from `schema.rs` to keep it under 500 lines (RC-100-43).

use super::Choice;

pub(super) const THEMES: &[Choice] = &[
    Choice {
        value: "system",
        label: "setting.appearance.theme.system",
    },
    Choice {
        value: "light",
        label: "setting.appearance.theme.light",
    },
    Choice {
        value: "dark",
        label: "setting.appearance.theme.dark",
    },
];

pub(super) const FILE_ICONS: &[Choice] = &[
    Choice {
        value: "native",
        label: "setting.appearance.file_icons.native",
    },
    Choice {
        value: "generic",
        label: "setting.appearance.file_icons.generic",
    },
    Choice {
        value: "off",
        label: "setting.appearance.file_icons.off",
    },
];

/// The quick search's keys; letters alone no longer go into the command line.
pub(super) const QUICK_SEARCH_KEYS: &[Choice] = &[
    Choice {
        value: "ctrl_alt",
        label: "setting.quick_search.keys.ctrl_alt",
    },
    Choice {
        value: "alt",
        label: "setting.quick_search.keys.alt",
    },
    Choice {
        value: "letters",
        label: "setting.quick_search.keys.letters",
    },
];
