//! The SFTP row (RC-080-60): whether names on an SFTP server that differ only in case are one
//! name. Detected by default - the backend asks the server once when it opens (`rc-remote`'s
//! `sftp/case.rs`) -, or fixed for every server where the probe is wrong. It decides whether
//! copying `Data` into `data/sub` is refused and whether a rename that only changes the case goes
//! through a temporary name. It holds for SFTP backends opened after the change.

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
