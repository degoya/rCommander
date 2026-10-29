//! The backups' row (RC-080-42, the owner's decision of 2026-10-28): how many backups under a
//! generated name the backup folder keeps; after each new one the oldest beyond it are removed,
//! a backup a person named never. Protected: lowering it deletes backups, which is a person's
//! decision, not an agent's.

use super::{SettingKind, SettingPage, SettingSpec};

/// How many automatic backups stay in the backup folder.
pub const BACKUP_KEEP_KEY: &str = "backup.keep";

pub(super) const KEEP: SettingSpec = SettingSpec {
    key: BACKUP_KEEP_KEY,
    page: SettingPage::Exchange,
    kind: SettingKind::Int {
        min: 1,
        max: 1000,
        default: 10,
    },
    label: "setting.backup.keep",
    help: "setting.backup.keep.help",
    protected: true,
    restart: false,
};
