//! The rows of the export (RC-080-19): the parts and the encryption chosen last, which the export
//! dialog starts with. The engine stores them after each export; the settings dialog shows them
//! on its Import/Export page. The encryption is protected: no agent may make the next export
//! unencrypted.

use super::{SettingKind, SettingPage, SettingSpec};

/// The settings go into the next export.
pub const EXPORT_SETTINGS_KEY: &str = "exchange.export.settings";
/// The connections, their folders and login scripts go into the next export.
pub const EXPORT_CONNECTIONS_KEY: &str = "exchange.export.connections";
/// The trusted host keys and certificates go into the next export.
pub const EXPORT_TRUST_KEY: &str = "exchange.export.trust";
/// The hotlist and the tabs go into the next export.
pub const EXPORT_HOTLIST_KEY: &str = "exchange.export.hotlist";
/// The next export is sealed with an export password.
pub const EXPORT_ENCRYPTED_KEY: &str = "exchange.export.encrypted";

/// A part, on by default.
const fn part(key: &'static str, label: &'static str, help: &'static str) -> SettingSpec {
    SettingSpec {
        key,
        page: SettingPage::Exchange,
        kind: SettingKind::Bool { default: true },
        label,
        help,
        protected: false,
        restart: false,
    }
}

pub(super) const EXPORT_SETTINGS: SettingSpec = part(
    EXPORT_SETTINGS_KEY,
    "setting.exchange.export.settings",
    "setting.exchange.export.settings.help",
);

pub(super) const EXPORT_CONNECTIONS: SettingSpec = part(
    EXPORT_CONNECTIONS_KEY,
    "setting.exchange.export.connections",
    "setting.exchange.export.connections.help",
);

pub(super) const EXPORT_TRUST: SettingSpec = part(
    EXPORT_TRUST_KEY,
    "setting.exchange.export.trust",
    "setting.exchange.export.trust.help",
);

pub(super) const EXPORT_HOTLIST: SettingSpec = part(
    EXPORT_HOTLIST_KEY,
    "setting.exchange.export.hotlist",
    "setting.exchange.export.hotlist.help",
);

pub(super) const EXPORT_ENCRYPTED: SettingSpec = SettingSpec {
    protected: true,
    ..part(
        EXPORT_ENCRYPTED_KEY,
        "setting.exchange.export.encrypted",
        "setting.exchange.export.encrypted.help",
    )
};
