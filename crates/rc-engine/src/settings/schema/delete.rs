//! Deleting where there is no recycle bin (owner, 2026-10-18; RC-080-17). Off, F8 and Delete
//! there end in `trash_unavailable` as before and Shift+Delete deletes for good; on, F8 and
//! Delete open the permanent delete's question, which says it deletes for good. Not protected:
//! the question still comes, and an MCP delete never follows it.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether F8 and Delete delete for good, after the question, where there is no recycle bin.
pub const PERMANENT_WITHOUT_TRASH_KEY: &str = "delete.permanent_without_trash";

pub(super) const PERMANENT_WITHOUT_TRASH: SettingSpec = SettingSpec {
    key: PERMANENT_WITHOUT_TRASH_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: false },
    label: "setting.delete.permanent_without_trash",
    help: "setting.delete.permanent_without_trash.help",
    protected: false,
    restart: false,
};
