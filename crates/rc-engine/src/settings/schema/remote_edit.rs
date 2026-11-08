//! The rows of the server files edited through copies (RC-110-01), on the Programs page beside
//! the editor and the viewer.
//!
//! - `remote_edit.upload`: what a save of a copy does - `ask` (the default) asks Upload or Not
//!   now each time, with a box that switches to `auto`; `auto` uploads at once. Total
//!   Commander's "Automatically re-upload changed files" (`AutoReUpload`); its old way, upload
//!   only after the editor closed, is not offered: a program started by the system gives no way
//!   to see it close.
//! - `remote_edit.check_server_changes`: before an upload the server's file is looked at, and a
//!   change since the fetch (size or time) or its absence asks first. On by default.
//! - `remote_edit.open_programs`: Enter on a server file that would run as a program (`PATHEXT`
//!   on Windows, an execute bit elsewhere) opens it instead of refusing. Off by default.
//!
//! All three are protected: whether a change goes to a server unasked, unchecked, or a program
//! fetched from one runs is a person's choice, never an agent's.

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// What a save of a copy does: `ask` or `auto`.
pub const REMOTE_EDIT_UPLOAD_KEY: &str = "remote_edit.upload";
/// Whether the server's file is looked at before an upload.
pub const REMOTE_EDIT_CHECK_KEY: &str = "remote_edit.check_server_changes";
/// Whether Enter opens a server file that would run as a program.
pub const REMOTE_EDIT_PROGRAMS_KEY: &str = "remote_edit.open_programs";

const UPLOADS: &[Choice] = &[
    Choice {
        value: "ask",
        label: "setting.remote_edit.upload.ask",
    },
    Choice {
        value: "auto",
        label: "setting.remote_edit.upload.auto",
    },
];

pub(super) const UPLOAD: SettingSpec = SettingSpec {
    key: REMOTE_EDIT_UPLOAD_KEY,
    page: SettingPage::Programs,
    kind: SettingKind::Choice {
        choices: UPLOADS,
        default: "ask",
    },
    label: "setting.remote_edit.upload",
    help: "setting.remote_edit.upload.help",
    protected: true,
    restart: false,
};

pub(super) const CHECK_SERVER_CHANGES: SettingSpec = SettingSpec {
    key: REMOTE_EDIT_CHECK_KEY,
    page: SettingPage::Programs,
    kind: SettingKind::Bool { default: true },
    label: "setting.remote_edit.check_server_changes",
    help: "setting.remote_edit.check_server_changes.help",
    protected: true,
    restart: false,
};

pub(super) const OPEN_PROGRAMS: SettingSpec = SettingSpec {
    key: REMOTE_EDIT_PROGRAMS_KEY,
    page: SettingPage::Programs,
    kind: SettingKind::Bool { default: false },
    label: "setting.remote_edit.open_programs",
    help: "setting.remote_edit.open_programs.help",
    protected: true,
    restart: false,
};
