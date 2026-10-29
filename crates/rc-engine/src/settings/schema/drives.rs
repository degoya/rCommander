//! Changing a panel's drive (RC-010-05, owner 2026-10-19): a drive the other panel shows opens
//! at the other panel's folder, as in Total Commander; `d:` in the command line does so even when
//! the panel is already on that drive (owner 2026-10-26, RC-010-29). On by default; not
//! protected.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether a drive the other panel shows opens at the other panel's folder.
pub const FOLLOW_OTHER_PANEL_KEY: &str = "drives.follow_other_panel";
/// Whether `d:` in the command line follows the other panel on the panel's own drive too.
pub const FOLLOW_ON_OWN_DRIVE_KEY: &str = "drives.follow_on_own_drive";

pub(super) const FOLLOW_OTHER_PANEL: SettingSpec = SettingSpec {
    key: FOLLOW_OTHER_PANEL_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.drives.follow_other_panel",
    help: "setting.drives.follow_other_panel.help",
    protected: false,
    restart: false,
};

pub(super) const FOLLOW_ON_OWN_DRIVE: SettingSpec = SettingSpec {
    key: FOLLOW_ON_OWN_DRIVE_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.drives.follow_on_own_drive",
    help: "setting.drives.follow_on_own_drive.help",
    protected: false,
    restart: false,
};
