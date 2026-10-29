//! What F10 does and whether quitting asks first (owner, 2026-10-18; RC-080-17), whether a
//! dialog's access keys are always underlined (RC-080-16), and whether the context menus show the
//! keys of their entries (RC-010-24), whether the connection manager opens with the keyboard in
//! its list (RC-050-15); and the person's deviations from the standard keymap
//! (RC-100-21). Only those are protected: a moved key changes what one key press of a person
//! runs, so only the person moves it (`keymap.set` is `GuiOnly`); an agent may switch the rest
//! as a person may, and none of them loosens a permission.

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// What F10 does: `close_and_quit` closes the open dialog as Escape does and quits without
/// one, `close_dialogs` only closes dialogs, `off` leaves the key alone.
pub const F10_KEY: &str = "keys.f10";
/// Whether quitting while transfers run or wait asks first.
pub const CONFIRM_QUIT_KEY: &str = "app.confirm_quit_with_transfers";
/// Whether the access keys of a dialog's buttons are always underlined; off, they show while
/// Alt is held, as in Windows.
pub const UNDERLINE_ACCESS_KEYS_KEY: &str = "keys.underline_access_keys";
/// Whether the menus rCommander draws show each entry's key beside it, as Windows menus do.
pub const CONTEXT_MENU_KEYS_KEY: &str = "keys.context_menu_shortcuts";
/// Whether the connection manager opens with the keyboard in its list, on the connection used
/// last; off, in its filter field.
pub const CONNECTIONS_FOCUS_LIST_KEY: &str = "connections.focus_list";
/// The deviations from the standard keymap, edited on the Keyboard page's Shortcuts tab.
pub const KEYMAP_KEY: &str = "keys.overrides";

const F10_CHOICES: &[Choice] = &[
    Choice {
        value: "close_and_quit",
        label: "setting.keys.f10.close_and_quit",
    },
    Choice {
        value: "close_dialogs",
        label: "setting.keys.f10.close_dialogs",
    },
    Choice {
        value: "off",
        label: "setting.keys.f10.off",
    },
];

pub(super) const F10: SettingSpec = SettingSpec {
    key: F10_KEY,
    page: SettingPage::Keyboard,
    kind: SettingKind::Choice {
        choices: F10_CHOICES,
        default: "close_and_quit",
    },
    label: "setting.keys.f10",
    help: "setting.keys.f10.help",
    protected: false,
    restart: false,
};

pub(super) const CONFIRM_QUIT: SettingSpec = SettingSpec {
    key: CONFIRM_QUIT_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: true },
    label: "setting.app.confirm_quit_with_transfers",
    help: "setting.app.confirm_quit_with_transfers.help",
    protected: false,
    restart: false,
};

pub(super) const UNDERLINE_ACCESS_KEYS: SettingSpec = SettingSpec {
    key: UNDERLINE_ACCESS_KEYS_KEY,
    page: SettingPage::Keyboard,
    kind: SettingKind::Bool { default: false },
    label: "setting.keys.underline_access_keys",
    help: "setting.keys.underline_access_keys.help",
    protected: false,
    restart: false,
};

pub(super) const CONTEXT_MENU_KEYS: SettingSpec = SettingSpec {
    key: CONTEXT_MENU_KEYS_KEY,
    page: SettingPage::Keyboard,
    kind: SettingKind::Bool { default: true },
    label: "setting.keys.context_menu_shortcuts",
    help: "setting.keys.context_menu_shortcuts.help",
    protected: false,
    restart: false,
};

pub(super) const CONNECTIONS_FOCUS_LIST: SettingSpec = SettingSpec {
    key: CONNECTIONS_FOCUS_LIST_KEY,
    page: SettingPage::Keyboard,
    kind: SettingKind::Bool { default: true },
    label: "setting.connections.focus_list",
    help: "setting.connections.focus_list.help",
    protected: false,
    restart: false,
};

pub(super) const KEYMAP: SettingSpec = SettingSpec {
    key: KEYMAP_KEY,
    page: SettingPage::Keyboard,
    kind: SettingKind::Keymap { default: "" },
    label: "setting.keys.overrides",
    help: "setting.keys.overrides.help",
    protected: true,
    restart: false,
};
