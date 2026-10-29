//! The fonts of the file lists, the terminal and the viewer (RC-010-17): a family and a size for
//! each, on the Appearance page. An empty family is the area's default; a family this machine
//! does not have falls back to it in the window, which alone knows the installed fonts. Beside
//! them, how a file name too long for its column is shortened and how long the pointer rests
//! before its tooltip shows (owner, 2026-10-19).

use super::{
    Choice, SettingKind, SettingPage, SettingSpec,
    viewer::{VIEWER_MAX_FONT_SIZE, VIEWER_MIN_FONT_SIZE},
};

/// The file lists' font (both panels, the tabs, the path line); empty for the interface font.
pub const LIST_FONT_KEY: &str = "appearance.list_font";
/// The file lists' font size in points.
pub const LIST_FONT_SIZE_KEY: &str = "appearance.list_font_size";
/// Where a file list's name too long for its column is cut: `end` or `middle`.
pub const NAME_CUT_KEY: &str = "appearance.name_cut";
/// How long, in milliseconds, the pointer rests on a shortened file name before its tooltip shows.
pub const NAME_TIP_DELAY_KEY: &str = "appearance.name_tip_delay";
/// Whether a hidden or system entry's icon is faded with a `!` over it, as in Total Commander.
pub const MARK_HIDDEN_KEY: &str = "appearance.mark_hidden";
/// The terminal's font; empty for Cascadia Mono (the system's monospace off Windows).
pub const TERMINAL_FONT_KEY: &str = "terminal.font";
/// The terminal's font size in points.
pub const TERMINAL_FONT_SIZE_KEY: &str = "terminal.font_size";
/// The viewer's font (text and hex, F3 and Ctrl+Q); empty for Cascadia Mono.
pub const VIEWER_FONT_KEY: &str = "viewer.font";
/// The viewer's font size in points, 6 to 72 (`VIEWER_MIN_FONT_SIZE`, `VIEWER_MAX_FONT_SIZE`).
pub const VIEWER_FONT_SIZE_KEY: &str = "viewer.font_size";

/// The smallest and the largest font size of the lists and the terminal, and the default (the
/// interface's).
pub const MIN_FONT_SIZE: i64 = 8;
pub const MAX_FONT_SIZE: i64 = 24;
pub const DEFAULT_FONT_SIZE: i64 = 13;

const fn family(key: &'static str, label: &'static str, help: &'static str) -> SettingSpec {
    SettingSpec {
        key,
        page: SettingPage::Appearance,
        kind: SettingKind::Font { default: "" },
        label,
        help,
        protected: false,
        restart: false,
    }
}

const fn size(key: &'static str, label: &'static str, help: &'static str) -> SettingSpec {
    size_in(key, label, help, MIN_FONT_SIZE, MAX_FONT_SIZE)
}

const fn size_in(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    min: i64,
    max: i64,
) -> SettingSpec {
    SettingSpec {
        key,
        page: SettingPage::Appearance,
        kind: SettingKind::Int {
            min,
            max,
            default: DEFAULT_FONT_SIZE,
        },
        label,
        help,
        protected: false,
        restart: false,
    }
}

pub(super) const LIST_FONT: SettingSpec = family(
    LIST_FONT_KEY,
    "setting.appearance.list_font",
    "setting.appearance.list_font.help",
);
pub(super) const LIST_FONT_SIZE: SettingSpec = size(
    LIST_FONT_SIZE_KEY,
    "setting.appearance.list_font_size",
    "setting.appearance.list_font_size.help",
);
const NAME_CUTS: &[Choice] = &[
    Choice {
        value: "end",
        label: "setting.appearance.name_cut.end",
    },
    Choice {
        value: "middle",
        label: "setting.appearance.name_cut.middle",
    },
];

pub(super) const NAME_CUT: SettingSpec = SettingSpec {
    key: NAME_CUT_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Choice {
        choices: NAME_CUTS,
        default: "end",
    },
    label: "setting.appearance.name_cut",
    help: "setting.appearance.name_cut.help",
    protected: false,
    restart: false,
};
pub(super) const NAME_TIP_DELAY: SettingSpec = SettingSpec {
    key: NAME_TIP_DELAY_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Int {
        min: 0,
        max: 5000,
        default: 500,
    },
    label: "setting.appearance.name_tip_delay",
    help: "setting.appearance.name_tip_delay.help",
    protected: false,
    restart: false,
};
pub(super) const MARK_HIDDEN: SettingSpec = SettingSpec {
    key: MARK_HIDDEN_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.appearance.mark_hidden",
    help: "setting.appearance.mark_hidden.help",
    protected: false,
    restart: false,
};
pub(super) const TERMINAL_FONT: SettingSpec = family(
    TERMINAL_FONT_KEY,
    "setting.terminal.font",
    "setting.terminal.font.help",
);
pub(super) const TERMINAL_FONT_SIZE: SettingSpec = size(
    TERMINAL_FONT_SIZE_KEY,
    "setting.terminal.font_size",
    "setting.terminal.font_size.help",
);
pub(super) const VIEWER_FONT: SettingSpec = family(
    VIEWER_FONT_KEY,
    "setting.viewer.font",
    "setting.viewer.font.help",
);
/// Wider than the other areas' sizes: the viewer's text size keys reach it (RC-020-12).
pub(super) const VIEWER_FONT_SIZE: SettingSpec = size_in(
    VIEWER_FONT_SIZE_KEY,
    "setting.viewer.font_size",
    "setting.viewer.font_size.help",
    VIEWER_MIN_FONT_SIZE,
    VIEWER_MAX_FONT_SIZE,
);
