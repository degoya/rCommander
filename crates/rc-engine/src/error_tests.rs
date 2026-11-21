use rc_db::{DbError, RetentionError};
use rc_vfs::VfsError;

use super::{
    CompareError, EngineError, ImportError, MultiRenameError, SessionError, SettingsError,
};
use crate::{ConnectionError, TermError};

/// One error of every variant. The match stops compiling when a variant is added: give it a
/// sample here and its code in `ALL_CODES`.
fn one_of_each_own() -> Vec<EngineError> {
    let samples = vec![
        EngineError::EntryNotFound {
            name: "x".to_owned(),
        },
        EngineError::CursorOutOfRange { index: 1, len: 0 },
        EngineError::BackendUnavailable,
        EngineError::VaultLocked,
        EngineError::DatabaseUnavailable,
        EngineError::GuiOnly {
            command: "hostkeys.answer",
        },
        EngineError::Database(DbError::NoDataDir),
        EngineError::HostKeyUnknown {
            host: "box".to_owned(),
            port: 22,
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: "SHA256:x".to_owned(),
        },
        EngineError::HostKeyChanged {
            host: "box".to_owned(),
            port: 22,
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: "SHA256:x".to_owned(),
            stored_fingerprint: "SHA256:y".to_owned(),
        },
        EngineError::HostKeyRevoked {
            host: "box".to_owned(),
            port: 22,
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: "SHA256:x".to_owned(),
        },
        EngineError::HostKeyStale,
        EngineError::HostKeyQuestionNotFound { question: 1 },
        EngineError::HostKeyNotChanged { question: 1 },
        EngineError::KnownHostNotFound { id: 1 },
        EngineError::KnownHostsUnreadable {
            path: "/x".to_owned(),
            source: std::io::Error::other("x"),
        },
        EngineError::Settings(SettingsError::RetentionUnknownTable {
            table: "settings".to_owned(),
        }),
        EngineError::Settings(SettingsError::RetentionOutOfRange {
            key: "retention.error_log.max_rows",
            value: 0,
            min: 100,
            max: 50_000,
        }),
        EngineError::MasterPasswordTooShort { min: 8 },
        EngineError::SecretEmpty,
        EngineError::KeyFormatUnknown,
        EngineError::KeyPassphraseNeeded,
        EngineError::KeyPassphraseWrong,
        EngineError::KeyImportOnly,
        EngineError::TotpSeedInvalid {
            reason: "x".to_owned(),
        },
        EngineError::KeyFileUnreadable {
            path: "/x".to_owned(),
            source: std::io::Error::other("x"),
        },
        EngineError::KeyPathRefused {
            path: r"\\server\share\key".to_owned(),
        },
        EngineError::Import(ImportError::SshConfigPathRefused {
            path: "/x".to_owned(),
        }),
        EngineError::Import(ImportError::SshConfigKeyMaterial {
            path: "/x".to_owned(),
        }),
        EngineError::Import(ImportError::SshConfigUnreadable {
            path: "/x".to_owned(),
            source: std::io::Error::other("x"),
        }),
        EngineError::Import(ImportError::SshConfigHostUnknown {
            alias: "x".to_owned(),
        }),
        EngineError::Import(ImportError::TcIniPathRefused {
            path: "/x".to_owned(),
        }),
        EngineError::Import(ImportError::TcIniUnreadable {
            path: "/x".to_owned(),
            source: std::io::Error::other("x"),
        }),
        EngineError::Import(ImportError::TcIniTooLarge {
            path: "/x".to_owned(),
            max: 1,
        }),
        EngineError::Import(ImportError::TcIniNotFound),
        EngineError::Import(ImportError::RdmPathRefused {
            path: "/x".to_owned(),
        }),
        EngineError::Import(ImportError::RdmUnreadable {
            path: "/x".to_owned(),
            source: std::io::Error::other("x"),
        }),
        EngineError::Import(ImportError::RdmTooLarge {
            path: "/x".to_owned(),
            max: 1,
        }),
        EngineError::Import(ImportError::RdmEncrypted {
            path: "/x".to_owned(),
        }),
        EngineError::Import(ImportError::RdmInvalid {
            path: "/x".to_owned(),
            line: 1,
        }),
        EngineError::Import(ImportError::RdmNotAnExport {
            path: "/x".to_owned(),
        }),
        EngineError::InvalidSearch {
            reason: "x".to_owned(),
        },
        EngineError::Cancelled,
        EngineError::Compare(CompareError::CompareNeedsFiles),
        EngineError::Compare(CompareError::CompareChanged {
            name: "a.txt".to_owned(),
        }),
        EngineError::Compare(CompareError::CompareNotEditable {
            name: "a.txt".to_owned(),
            reason: "too_large",
        }),
        EngineError::Compare(CompareError::CompareBlockNotFound { block: 3 }),
        EngineError::Compare(CompareError::SyncDeletesUnconfirmed { count: 1 }),
        EngineError::Compare(CompareError::SyncActionInvalid {
            row: 1,
            action: crate::sync::SyncAction::DeleteLeft,
        }),
        EngineError::Compare(CompareError::SyncRowNotFound {
            path: "x".to_owned(),
        }),
        EngineError::OpenFailed {
            name: "setup.exe".to_owned(),
        },
        EngineError::DriveNotFound {
            drive: "Q:".to_owned(),
        },
        EngineError::DriveNotReady {
            drive: "D:".to_owned(),
        },
        EngineError::Settings(SettingsError::SettingUnknown {
            key: "x.y".to_owned(),
        }),
        EngineError::Settings(SettingsError::SettingProtected {
            key: "mcp.policy".to_owned(),
        }),
        EngineError::Settings(SettingsError::SettingInvalid {
            key: "appearance.theme".to_owned(),
            reason: "expected one of system, light, dark".to_owned(),
        }),
        EngineError::SensitivePath {
            path: "/x".to_owned(),
            reason: "x",
        },
        EngineError::EndpointChangeNeedsGui { id: 1 },
        EngineError::MultiRename(MultiRenameError::Busy),
        EngineError::MultiRename(MultiRenameError::NoChanges),
        EngineError::MultiRename(MultiRenameError::NothingToUndo),
        EngineError::MultiRename(MultiRenameError::UndoIncomplete),
        EngineError::MultiRename(MultiRenameError::UndoMismatch {
            mismatches: Vec::new(),
        }),
        EngineError::MultiRename(MultiRenameError::PresetInvalid {
            reason: "empty name".to_owned(),
        }),
        EngineError::MultiRename(MultiRenameError::PresetNotFound {
            name: "Photos".to_owned(),
        }),
        EngineError::Session(SessionError::ConnectionNotSsh {
            id: crate::ConnectionId(1),
        }),
        EngineError::Session(SessionError::ExecInvalid {
            reason: "empty command".to_owned(),
        }),
        EngineError::Session(SessionError::SessionPrefsInvalid {
            reason: "x".to_owned(),
        }),
        EngineError::PasswordNotStored {
            connection: crate::ConnectionId(1),
        },
        EngineError::Session(SessionError::FtpConnectionNotFound {
            connection: "nas".to_owned(),
        }),
        EngineError::Session(SessionError::FtpConnectionAmbiguous {
            connection: "nas".to_owned(),
            count: 2,
        }),
        EngineError::McpQuestionNotFound { question: 1 },
        EngineError::McpAnswerNotOffered { question: 1 },
        EngineError::CommandLineInvalid { reason: "empty" },
        EngineError::CommandLineLocalOnly,
        EngineError::CommandLineFailed {
            source: std::io::Error::other("no shell"),
        },
        EngineError::FilterInvalid {
            reason: "control characters",
        },
        EngineError::ArchiveReadOnly,
        EngineError::NotAnArchive {
            name: "notes.txt".to_owned(),
        },
        EngineError::PropertyInvalid {
            reason: "a link needs its place",
        },
        EngineError::CommentNotEncodable {
            encoding: "Windows-1252",
        },
        EngineError::Session(SessionError::SftpConnectionNotFound {
            connection: "web".to_owned(),
        }),
        EngineError::Session(SessionError::SftpConnectionAmbiguous {
            connection: "web".to_owned(),
            count: 2,
        }),
        EngineError::SavedQueueNotFound { saved: 3 },
        EngineError::SavedQueueChanged { saved: 3 },
        EngineError::QueueListEmpty,
        EngineError::PanelChanged { panel: "left" },
        EngineError::ThemeUnknown { id: "neon".into() },
        EngineError::ThemeWrongMode {
            id: "nord-dark".into(),
            mode: "light",
        },
        EngineError::ThemeBuiltin {
            id: "fluent-dark".into(),
        },
        EngineError::ThemeInUse { id: "own-1".into() },
        EngineError::ThemeNameTaken { name: "N".into() },
        EngineError::ThemeInvalid {
            reason: "a theme needs a name".into(),
        },
    ];
    for sample in &samples {
        match sample {
            EngineError::Vfs(_)
            | EngineError::Term(_)
            | EngineError::Connection(_)
            | EngineError::Ops(_)
            | EngineError::Agent(_)
            | EngineError::Checksum(_)
            | EngineError::Backup(_)
            | EngineError::Exchange(_)
            | EngineError::Plugin(_)
            | EngineError::SecretSource(_)
            | EngineError::Rename(_)
            | EngineError::Tabs(_)
            | EngineError::Update(_)
            | EngineError::Workspace(_)
            | EngineError::Inventory(_)
            | EngineError::ColumnSet(_)
            | EngineError::UserCommand(_)
            | EngineError::RemoteEdit(_)
            | EngineError::Forward(_)
            | EngineError::Hop { .. }
            | EngineError::MultiRename(MultiRenameError::Busy)
            | EngineError::MultiRename(MultiRenameError::NoChanges)
            | EngineError::MultiRename(MultiRenameError::NothingToUndo)
            | EngineError::MultiRename(MultiRenameError::UndoIncomplete)
            | EngineError::MultiRename(MultiRenameError::UndoMismatch { .. })
            | EngineError::MultiRename(MultiRenameError::PresetInvalid { .. })
            | EngineError::MultiRename(MultiRenameError::PresetNotFound { .. })
            | EngineError::Ssh(_)
            | EngineError::LoginScript(_)
            | EngineError::LoginScriptInvalid(_)
            | EngineError::Ftp(_)
            | EngineError::Certificate(_)
            | EngineError::Sftp(_)
            | EngineError::NetworkDrive(_)
            | EngineError::EntryNotFound { .. }
            | EngineError::CursorOutOfRange { .. }
            | EngineError::BackendUnavailable
            | EngineError::VaultLocked
            | EngineError::DatabaseUnavailable
            | EngineError::GuiOnly { .. }
            | EngineError::Database(_)
            | EngineError::HostKeyUnknown { .. }
            | EngineError::HostKeyChanged { .. }
            | EngineError::HostKeyRevoked { .. }
            | EngineError::HostKeyStale
            | EngineError::HostKeyQuestionNotFound { .. }
            | EngineError::HostKeyNotChanged { .. }
            | EngineError::KnownHostNotFound { .. }
            | EngineError::KnownHostsUnreadable { .. }
            | EngineError::Vault(_)
            | EngineError::MasterPasswordTooShort { .. }
            | EngineError::SecretEmpty
            | EngineError::KeyFormatUnknown
            | EngineError::KeyPassphraseNeeded
            | EngineError::KeyPassphraseWrong
            | EngineError::KeyImportOnly
            | EngineError::TotpSeedInvalid { .. }
            | EngineError::KeyFileUnreadable { .. }
            | EngineError::KeyPathRefused { .. }
            | EngineError::Import(ImportError::SshConfigPathRefused { .. })
            | EngineError::Import(ImportError::SshConfigKeyMaterial { .. })
            | EngineError::Import(ImportError::SshConfigUnreadable { .. })
            | EngineError::Import(ImportError::SshConfigHostUnknown { .. })
            | EngineError::InvalidSearch { .. }
            | EngineError::Cancelled
            | EngineError::Compare(CompareError::CompareNeedsFiles)
            | EngineError::Compare(CompareError::CompareChanged { .. })
            | EngineError::Compare(CompareError::CompareNotEditable { .. })
            | EngineError::Compare(CompareError::CompareBlockNotFound { .. })
            | EngineError::Compare(CompareError::SyncDeletesUnconfirmed { .. })
            | EngineError::Compare(CompareError::SyncActionInvalid { .. })
            | EngineError::Compare(CompareError::SyncRowNotFound { .. })
            | EngineError::SensitivePath { .. }
            | EngineError::EndpointChangeNeedsGui { .. }
            | EngineError::Session(SessionError::ConnectionNotSsh { .. })
            | EngineError::Session(SessionError::ExecInvalid { .. })
            | EngineError::Session(SessionError::SessionPrefsInvalid { .. })
            | EngineError::PasswordNotStored { .. }
            | EngineError::Session(SessionError::FtpConnectionNotFound { .. })
            | EngineError::Session(SessionError::FtpConnectionAmbiguous { .. }) => {}
            EngineError::Settings(SettingsError::RetentionUnknownTable { .. })
            | EngineError::Settings(SettingsError::RetentionOutOfRange { .. }) => {}
            EngineError::OpenFailed { .. } => {}
            EngineError::DriveNotFound { .. } | EngineError::DriveNotReady { .. } => {}
            EngineError::Settings(SettingsError::SettingUnknown { .. })
            | EngineError::Settings(SettingsError::SettingProtected { .. })
            | EngineError::Settings(SettingsError::SettingInvalid { .. })
            | EngineError::McpQuestionNotFound { .. }
            | EngineError::McpAnswerNotOffered { .. } => {}
            EngineError::CommandLineInvalid { .. }
            | EngineError::CommandLineLocalOnly
            | EngineError::CommandLineFailed { .. }
            | EngineError::FilterInvalid { .. } => {}
            EngineError::ArchiveReadOnly | EngineError::NotAnArchive { .. } => {}
            EngineError::PropertyInvalid { .. } | EngineError::CommentNotEncodable { .. } => {}
            EngineError::Session(SessionError::SftpConnectionNotFound { .. })
            | EngineError::Session(SessionError::SftpConnectionAmbiguous { .. }) => {}
            EngineError::SavedQueueNotFound { .. }
            | EngineError::SavedQueueChanged { .. }
            | EngineError::QueueListEmpty => {}
            EngineError::PanelChanged { .. } => {}
            EngineError::ThemeUnknown { .. }
            | EngineError::ThemeWrongMode { .. }
            | EngineError::ThemeBuiltin { .. }
            | EngineError::ThemeInUse { .. }
            | EngineError::ThemeNameTaken { .. }
            | EngineError::ThemeInvalid { .. } => {}
            EngineError::Import(ImportError::TcIniPathRefused { .. })
            | EngineError::Import(ImportError::TcIniUnreadable { .. })
            | EngineError::Import(ImportError::TcIniTooLarge { .. })
            | EngineError::Import(ImportError::TcIniNotFound) => {}
            EngineError::Import(ImportError::RdmPathRefused { .. })
            | EngineError::Import(ImportError::RdmUnreadable { .. })
            | EngineError::Import(ImportError::RdmTooLarge { .. })
            | EngineError::Import(ImportError::RdmEncrypted { .. })
            | EngineError::Import(ImportError::RdmInvalid { .. })
            | EngineError::Import(ImportError::RdmNotAnExport { .. }) => {}
        }
    }
    samples
}

#[test]
fn all_codes_lists_every_own_code_once() {
    let codes: Vec<&str> = one_of_each_own().iter().map(EngineError::code).collect();
    assert_eq!(codes, EngineError::ALL_CODES);
}

// The wrapped failures keep their codes; a file of its own to stay under 500 lines.
#[path = "error_wrap_tests.rs"]
mod wrap;

// Every own code and text against the table written before the split into groups (RC-110-35).
#[path = "error_table_tests.rs"]
mod table;
