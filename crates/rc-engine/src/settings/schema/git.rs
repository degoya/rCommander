//! The git status in the file lists (RC-100-97, owner 2026-11-07). Where git may run is off by
//! default and protected: nothing starts `git` unasked, not even when an agent navigates; the
//! person switches it on. The program is protected too - a program an agent could name would be
//! code execution. Whether ignored entries are dimmed and the branch is shown are an agent's to
//! switch.

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// Where git runs for the panels' folders: `off`, `local`, `local_network`.
pub const GIT_STATUS_KEY: &str = "panels.git_status";
/// Whether ignored entries are asked for and dimmed.
pub const GIT_IGNORED_KEY: &str = "panels.git_ignored";
/// Whether the branch stands in the panel's status line.
pub const GIT_BRANCH_KEY: &str = "panels.git_branch";
/// The git program; empty for the first one of the `PATH`.
pub const GIT_PROGRAM_KEY: &str = "panels.git_program";

const SCOPES: &[Choice] = &[
    Choice {
        value: "off",
        label: "setting.panels.git_status.off",
    },
    Choice {
        value: "local",
        label: "setting.panels.git_status.local",
    },
    Choice {
        value: "local_network",
        label: "setting.panels.git_status.local_network",
    },
];

pub(super) const STATUS: SettingSpec = SettingSpec {
    key: GIT_STATUS_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Choice {
        choices: SCOPES,
        default: "off",
    },
    label: "setting.panels.git_status",
    help: "setting.panels.git_status.help",
    protected: true,
    restart: false,
};

pub(super) const IGNORED: SettingSpec = SettingSpec {
    key: GIT_IGNORED_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.panels.git_ignored",
    help: "setting.panels.git_ignored.help",
    protected: false,
    restart: false,
};

pub(super) const BRANCH: SettingSpec = SettingSpec {
    key: GIT_BRANCH_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.panels.git_branch",
    help: "setting.panels.git_branch.help",
    protected: false,
    restart: false,
};

pub(super) const PROGRAM: SettingSpec = SettingSpec {
    key: GIT_PROGRAM_KEY,
    page: SettingPage::Programs,
    kind: SettingKind::Path { default: "" },
    label: "setting.panels.git_program",
    help: "setting.panels.git_program.help",
    protected: true,
    restart: false,
};
