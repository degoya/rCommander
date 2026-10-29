//! "Browse..." beside the path fields (RC-010-18): the system's own dialog for a file or a folder
//! (`platform/file_dialog.rs`), with the kind, the file types and the start folder each field
//! wants. The path chosen goes into the field as the field's own input message - the same way,
//! and through the same checks, as a typed one; a cancelled dialog changes nothing. Only the
//! window opens the dialog: no command, no MCP tool.
//!
//! Remote paths are not chosen here: a connection's start folder is on the server and has no
//! button, as has the target of F5 and F6 (the other panel, as in Total Commander).

use std::path::PathBuf;

use iced::Element;
use rc_engine::settings::{SettingPage, SettingSpec};

use super::{
    App, Effect, Message,
    checksum::ChecksumMsg,
    connections::{ConnMsg, Edit, ImportMsg, RdmImportMsg, TcImportMsg},
    database_dialog::DatabaseMsg,
    multi_rename::{Field, MrMsg},
    quick_connect::QuickMsg,
    settings::{PluginsMsg, SettingsMessage},
};
use crate::{
    i18n::t,
    platform::file_dialog::{self, Fallback, Filter, Kind, Request},
    theme::{icon::Icon, layout},
};

/// A path field with a "Browse..." button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathField {
    /// The connection manager's private key file.
    ConnectionKey,
    /// The quick connection's private key file.
    QuickKey,
    /// The database dialog: the file a backup goes to, the backup to restore.
    Backup,
    Restore,
    /// The database dialog: the file an export goes to, the export to import (RC-080-19).
    ConfigExport,
    ConfigImport,
    /// A path setting of the settings dialog: a start folder, the viewer or the editor.
    Setting(&'static SettingSpec),
    /// The import's `ssh_config` file.
    SshConfig,
    /// The two files of the import from Total Commander.
    Wincmd,
    WcxFtp,
    /// The export of the import from Remote Desktop Manager.
    RdmExport,
    /// The multi-rename log file.
    RenameLog,
    /// The folder split and combine write into.
    ChecksumTarget,
    /// A plugin package to install (RC-100-09).
    PluginPackage,
}

impl PathField {
    /// The dialog for this field, which holds `current`.
    pub fn request(self, current: String) -> Request {
        let all = Filter {
            label: t!("browse.filter.all"),
            spec: "*.*",
        };
        let (kind, filters, fallback) = match self {
            Self::ConnectionKey | Self::QuickKey => (
                Kind::Open,
                vec![
                    Filter {
                        label: t!("browse.filter.keys"),
                        spec: "id_*;*.pem;*.key;*.ppk",
                    },
                    all,
                ],
                Fallback::SshFolder,
            ),
            Self::Backup | Self::Restore => {
                let kind = if self == Self::Backup {
                    Kind::Save
                } else {
                    Kind::Open
                };
                let backups = Filter {
                    label: t!("browse.filter.database"),
                    spec: "*.db",
                };
                (kind, vec![backups, all], Fallback::Dialog)
            }
            Self::ConfigExport | Self::ConfigImport => {
                let kind = if self == Self::ConfigExport {
                    Kind::Save
                } else {
                    Kind::Open
                };
                let exports = Filter {
                    label: t!("browse.filter.rcexport"),
                    spec: "*.rcexport",
                };
                (kind, vec![exports, all], Fallback::Dialog)
            }
            Self::Setting(spec) if spec.page == SettingPage::Programs => (
                Kind::Open,
                vec![
                    Filter {
                        label: t!("browse.filter.programs"),
                        spec: "*.exe;*.cmd;*.bat;*.com",
                    },
                    all,
                ],
                Fallback::Dialog,
            ),
            Self::Setting(_) | Self::ChecksumTarget => (Kind::Folder, Vec::new(), Fallback::Dialog),
            // An ssh_config file has no ending of its own.
            Self::SshConfig => (Kind::Open, vec![all], Fallback::SshFolder),
            Self::Wincmd => (
                Kind::Open,
                vec![
                    Filter {
                        label: t!("browse.filter.wincmd"),
                        spec: "wincmd.ini",
                    },
                    all,
                ],
                Fallback::Dialog,
            ),
            Self::WcxFtp => (
                Kind::Open,
                vec![
                    Filter {
                        label: t!("browse.filter.wcx_ftp"),
                        spec: "wcx_ftp.ini",
                    },
                    all,
                ],
                Fallback::Dialog,
            ),
            Self::RdmExport => (
                Kind::Open,
                vec![
                    Filter {
                        label: t!("browse.filter.rdm"),
                        spec: "*.rdm",
                    },
                    all,
                ],
                Fallback::Dialog,
            ),
            Self::PluginPackage => (
                Kind::Open,
                vec![
                    Filter {
                        label: t!("browse.filter.plugins"),
                        spec: "*.rcplug",
                    },
                    all,
                ],
                Fallback::Dialog,
            ),
            Self::RenameLog => (
                Kind::Save,
                vec![
                    Filter {
                        label: t!("browse.filter.logs"),
                        spec: "*.log;*.txt",
                    },
                    all,
                ],
                Fallback::Dialog,
            ),
        };
        Request {
            kind,
            filters,
            current,
            fallback,
        }
    }

    /// The message typing `path` into the field sends.
    fn typed(self, path: String) -> Message {
        match self {
            Self::ConnectionKey => Message::Connections(ConnMsg::Edit(Edit::KeyPath(path))),
            Self::QuickKey => Message::QuickConnect(QuickMsg::KeyPath(path)),
            Self::Backup | Self::Restore | Self::ConfigExport | Self::ConfigImport => {
                Message::Database(DatabaseMsg::Path(path))
            }
            Self::Setting(spec) => Message::Settings(SettingsMessage::Edit(spec.key, path)),
            Self::SshConfig => Message::Connections(ConnMsg::Import(ImportMsg::Path(path))),
            Self::Wincmd => Message::Connections(ConnMsg::TcImport(TcImportMsg::Wincmd(path))),
            Self::WcxFtp => Message::Connections(ConnMsg::TcImport(TcImportMsg::Ftp(path))),
            Self::RdmExport => Message::Connections(ConnMsg::RdmImport(RdmImportMsg::Path(path))),
            Self::RenameLog => Message::MultiRename(MrMsg::Text(Field::LogPath, path)),
            Self::ChecksumTarget => Message::Checksum(ChecksumMsg::Target(path)),
            Self::PluginPackage => {
                Message::Settings(SettingsMessage::Plugins(PluginsMsg::Path(path)))
            }
        }
    }
}

/// The button and the dialog's answer.
#[derive(Debug, Clone)]
pub enum BrowseMsg {
    /// "Browse..." beside the field, which holds the text.
    Open(PathField, String),
    /// The dialog closed: the path chosen, `None` when it was cancelled.
    Picked(PathField, Option<PathBuf>),
}

/// "Browse..." for `field`, which holds `current`; disabled while the field takes no input, not
/// drawn where the system has no file dialog (`docs/design.md`, Buttons).
pub fn button<'a>(field: PathField, current: &str, enabled: bool) -> Option<Element<'a, Message>> {
    file_dialog::AVAILABLE.then(|| {
        let open = Message::Browse(BrowseMsg::Open(field, current.to_owned()));
        layout::secondary_icon_button(Icon::Open, t!("browse.button"), enabled.then_some(open))
    })
}

/// What a closed dialog leaves: the field's own input with the path chosen; nothing when it was
/// cancelled, or for a path that is not Unicode (no field could hold it).
fn chosen(field: PathField, path: Option<PathBuf>) -> Option<Message> {
    let path = path?.into_os_string().into_string().ok()?;
    Some(field.typed(path))
}

impl App {
    pub(super) fn on_browse(&mut self, message: BrowseMsg) -> Vec<Effect> {
        match message {
            BrowseMsg::Open(field, current) => {
                let dialog = file_dialog::pick(field.request(current))
                    .map(move |path| Message::Browse(BrowseMsg::Picked(field, path)));
                vec![Effect::Task(dialog)]
            }
            BrowseMsg::Picked(field, path) => match chosen(field, path) {
                Some(message) => self.apply(message),
                None => Vec::new(),
            },
        }
    }
}

#[cfg(test)]
#[path = "browse_tests.rs"]
mod tests;
