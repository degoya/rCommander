//! The Mac's own ways (RC-100-49, after the contribution in PR #2): whether rCommander looks like
//! a Mac app - macOS's greys and accent, San Francisco and Menlo, the transparent title bar - or
//! like it does on Windows and Linux; and whether Cmd is the keymap's Ctrl. Both on by default
//! and both without effect on other systems. The look is chosen at the start (the colours and
//! the window's title bar are made once); the keys follow a change at once. Not protected:
//! neither loosens a permission.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether macOS draws with the Mac's colours, fonts and title bar; off, the Fluent look.
pub const MACOS_LOOK_KEY: &str = "appearance.macos_look";
/// Whether Cmd is the keymap's Ctrl on macOS; off, a press with Cmd is left to the system.
pub const MACOS_CMD_AS_CTRL_KEY: &str = "keys.macos_cmd_as_ctrl";

pub(super) const MACOS_LOOK: SettingSpec = SettingSpec {
    key: MACOS_LOOK_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.appearance.macos_look",
    help: "setting.appearance.macos_look.help",
    protected: false,
    restart: true,
};

pub(super) const MACOS_CMD_AS_CTRL: SettingSpec = SettingSpec {
    key: MACOS_CMD_AS_CTRL_KEY,
    page: SettingPage::Keyboard,
    kind: SettingKind::Bool { default: true },
    label: "setting.keys.macos_cmd_as_ctrl",
    help: "setting.keys.macos_cmd_as_ctrl.help",
    protected: false,
    restart: false,
};
