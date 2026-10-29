//! The page Appearance: the large dialogs in windows of their own (RC-080-15, owner 2026-10-18),
//! and the question after a lost graphics device (owner 2026-10-22).
//!
//! On, the connection manager, find files, multi-rename and compare/synchronize open as native
//! windows beside the main window - title bar, close X, moved and resized, their place kept;
//! multi-rename modal, the others modeless. Off, they lie over the main window as before. The
//! settings always have their window, the queue has its own switch (`queue.window_own`). Not
//! protected: either way the same dialog does the same work.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether the large dialogs open in windows of their own.
pub const DIALOG_WINDOWS_KEY: &str = "appearance.dialog_windows";

pub(super) const DIALOG_WINDOWS: SettingSpec = SettingSpec {
    key: DIALOG_WINDOWS_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.appearance.dialog_windows",
    help: "setting.appearance.dialog_windows.help",
    protected: false,
    restart: false,
};

/// Whether a lost graphics device asks to restart rCommander. On by default; off, the window
/// stays as it is until it is closed. Not protected: it only asks.
pub const GPU_LOSS_NOTICE_KEY: &str = "appearance.gpu_loss_notice";

pub(super) const GPU_LOSS_NOTICE: SettingSpec = SettingSpec {
    key: GPU_LOSS_NOTICE_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.appearance.gpu_loss_notice",
    help: "setting.appearance.gpu_loss_notice.help",
    protected: false,
    restart: false,
};
