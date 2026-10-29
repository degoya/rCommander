//! A panel's drive line (RC-010-22): whether it shows the volume name in brackets and the two
//! buttons `\` and `..` at its end, as Total Commander does. Both on by default; neither is
//! protected.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether the drive line ends with the buttons to the drive's root and one folder up.
pub const DRIVE_LINE_BUTTONS_KEY: &str = "panels.drive_line_buttons";
/// Whether the drive line shows the volume name (a server's or distribution's name) in brackets.
pub const DRIVE_LINE_VOLUME_KEY: &str = "panels.drive_line_volume";

pub(super) const BUTTONS: SettingSpec = SettingSpec {
    key: DRIVE_LINE_BUTTONS_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.panels.drive_line_buttons",
    help: "setting.panels.drive_line_buttons.help",
    protected: false,
    restart: false,
};

pub(super) const VOLUME: SettingSpec = SettingSpec {
    key: DRIVE_LINE_VOLUME_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.panels.drive_line_volume",
    help: "setting.panels.drive_line_volume.help",
    protected: false,
    restart: false,
};
