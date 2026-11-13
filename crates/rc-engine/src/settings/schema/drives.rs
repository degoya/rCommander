//! Changing a panel's drive (RC-010-05, owner 2026-10-19): a drive the other panel shows opens
//! at the other panel's folder, as in Total Commander; `d:` in the command line does so even when
//! the panel is already on that drive (owner 2026-10-26, RC-010-29). On by default; not
//! protected. Coming back to the window from another program reads the panels on local drives
//! again (owner 2026-11-01, RC-100-69): on by default; network drives and shares only with their
//! own switch, off by default. Neither is protected. Portable devices (phones over MTP) in the
//! drive bar (RC-100-61, owner 2026-11-01): on by default; off, the system is not asked for them.
//! While a copy or move onto a device runs, the panels say so (RC-110-16, `device_transfer.rs`):
//! on by default, not protected.

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

/// Whether coming back from another program reads the panels on local drives again.
pub const REFRESH_ON_FOCUS_KEY: &str = "panels.refresh_on_focus";
/// Whether that also reads panels on network drives and shares again.
pub const REFRESH_ON_FOCUS_NETWORK_KEY: &str = "panels.refresh_on_focus_network";

pub(super) const REFRESH_ON_FOCUS: SettingSpec = SettingSpec {
    key: REFRESH_ON_FOCUS_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.panels.refresh_on_focus",
    help: "setting.panels.refresh_on_focus.help",
    protected: false,
    restart: false,
};

pub(super) const REFRESH_ON_FOCUS_NETWORK: SettingSpec = SettingSpec {
    key: REFRESH_ON_FOCUS_NETWORK_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.panels.refresh_on_focus_network",
    help: "setting.panels.refresh_on_focus_network.help",
    protected: false,
    restart: false,
};

/// Whether the drive bar lists portable devices (phones, tablets, cameras over MTP).
pub const PORTABLE_DEVICES_KEY: &str = "drives.portable_devices";

pub(super) const PORTABLE_DEVICES: SettingSpec = SettingSpec {
    key: PORTABLE_DEVICES_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.drives.portable_devices",
    help: "setting.drives.portable_devices.help",
    protected: false,
    restart: false,
};

/// Whether the panels say that a transfer onto a portable device is running (RC-110-16).
pub const DEVICE_TRANSFER_HINT_KEY: &str = "panels.device_transfer_hint";

pub(super) const DEVICE_TRANSFER_HINT: SettingSpec = SettingSpec {
    key: DEVICE_TRANSFER_HINT_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.panels.device_transfer_hint",
    help: "setting.panels.device_transfer_hint.help",
    protected: false,
    restart: false,
};
