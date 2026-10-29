//! What Enter and a double click do on a checksum file (RC-020-10, owner 2026-10-19): on, a
//! `.sfv`, `.md5`, `.sha256` and the rest are checked as Ctrl+Shift+F10 checks them; off, the
//! system opens them as any other file. On by default; shown beside `archive.enter_opens`, the
//! other setting of what Enter does with a file. Not protected: it starts a read-only check.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether Enter and a double click check a checksum file instead of opening it.
pub const CHECKSUM_ENTER_KEY: &str = "checksum.enter_verifies";

pub(super) const ENTER_VERIFIES: SettingSpec = SettingSpec {
    key: CHECKSUM_ENTER_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.checksum.enter_verifies",
    help: "setting.checksum.enter_verifies.help",
    protected: false,
    restart: false,
};
