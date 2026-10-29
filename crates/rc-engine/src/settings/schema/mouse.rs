//! What a click in a file list does with the marks (RC-010-02, owner 2026-10-18): a plain click
//! moves the cursor only, as in Total Commander, or clears the marks too, as in Windows Explorer.
//! Off by default. Whether a tab is dragged to another place in its tab bar (RC-010-24), as in
//! Total Commander: on by default. Neither is protected.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether a plain click in a file list clears the marks, as in Windows Explorer.
pub const CLICK_CLEARS_MARKS_KEY: &str = "mouse.click_clears_marks";

pub(super) const CLICK_CLEARS_MARKS: SettingSpec = SettingSpec {
    key: CLICK_CLEARS_MARKS_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.mouse.click_clears_marks",
    help: "setting.mouse.click_clears_marks.help",
    protected: false,
    restart: false,
};

/// Whether a tab is dragged to another place in its tab bar; off, a press on a tab only shows it.
pub const TAB_DRAG_KEY: &str = "tabs.drag_reorder";

pub(super) const TAB_DRAG: SettingSpec = SettingSpec {
    key: TAB_DRAG_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.tabs.drag_reorder",
    help: "setting.tabs.drag_reorder.help",
    protected: false,
    restart: false,
};
