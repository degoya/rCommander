//! The archive settings, shown on the page Folders: whether Enter opens an archive in the panel
//! (RC-030-02), and how much memory a decoder may set aside for an xz or 7z dictionary
//! (RC-080-35). The memory limit refuses a header that asks for more before anything is
//! reserved, so a crafted archive of a few bytes cannot take gigabytes when it is opened; 16 MiB
//! to 4 GiB, 256 MiB by default, more than any preset of xz and 7-Zip uses. Neither is protected:
//! the limit only decides whether an archive opens, never what it may write.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether Enter opens an archive in the panel (on) or hands it to the system.
pub const ARCHIVE_ENTER_KEY: &str = "archive.enter_opens";
/// The memory a decoder may set aside for its dictionary, in MiB (RC-080-35).
pub const DECODER_MEMORY_KEY: &str = "archive.decoder_memory_mib";

pub(super) const ENTER_OPENS: SettingSpec = SettingSpec {
    key: ARCHIVE_ENTER_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.archive.enter_opens",
    help: "setting.archive.enter_opens.help",
    protected: false,
    restart: false,
};

pub(super) const DECODER_MEMORY: SettingSpec = SettingSpec {
    key: DECODER_MEMORY_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Int {
        min: 16,
        max: 4096,
        default: 256,
    },
    label: "setting.archive.decoder_memory_mib",
    help: "setting.archive.decoder_memory_mib.help",
    protected: false,
    restart: false,
};
