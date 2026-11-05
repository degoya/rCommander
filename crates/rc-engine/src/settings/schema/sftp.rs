//! The SFTP row (RC-080-60): whether names on an SFTP server that differ only in case are one
//! name. Detected by default - the backend asks the server once when it opens (`rc-remote`'s
//! `sftp/case.rs`) -, or fixed for every server where the probe is wrong. It decides whether
//! copying `Data` into `data/sub` is refused and whether a rename that only changes the case goes
//! through a temporary name. It holds for SFTP backends opened after the change.
//!
//! The rights a completed upload gets (RC-080-67): an upload creates its file owner-only, so no
//! one else on the server reads it while it runs, and gives it these rights once it is whole -
//! `rw-r--r--` by default, as a server's usual umask would; the copy's kept attributes come after
//! it. rCommander's own rule: Total Commander has no such row.

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// Whether an SFTP server tells names apart by case: `auto`, `sensitive` or `insensitive`.
pub const SFTP_CASE_KEY: &str = "sftp.case_sensitivity";

const CASES: &[Choice] = &[
    Choice {
        value: "auto",
        label: "setting.sftp.case_sensitivity.auto",
    },
    Choice {
        value: "sensitive",
        label: "setting.sftp.case_sensitivity.sensitive",
    },
    Choice {
        value: "insensitive",
        label: "setting.sftp.case_sensitivity.insensitive",
    },
];

/// The rights a completed SFTP upload gets: `644`, `664` or `600` (octal).
pub const SFTP_UPLOAD_RIGHTS_KEY: &str = "sftp.upload_rights";

const RIGHTS: &[Choice] = &[
    Choice {
        value: "644",
        label: "setting.sftp.upload_rights.644",
    },
    Choice {
        value: "664",
        label: "setting.sftp.upload_rights.664",
    },
    Choice {
        value: "600",
        label: "setting.sftp.upload_rights.600",
    },
];

pub(super) const UPLOAD_RIGHTS: SettingSpec = SettingSpec {
    key: SFTP_UPLOAD_RIGHTS_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Choice {
        choices: RIGHTS,
        default: "644",
    },
    label: "setting.sftp.upload_rights",
    help: "setting.sftp.upload_rights.help",
    protected: false,
    restart: false,
};

pub(super) const CASE: SettingSpec = SettingSpec {
    key: SFTP_CASE_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Choice {
        choices: CASES,
        default: "auto",
    },
    label: "setting.sftp.case_sensitivity",
    help: "setting.sftp.case_sensitivity.help",
    protected: false,
    restart: false,
};
