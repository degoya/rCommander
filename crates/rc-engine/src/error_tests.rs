use rc_db::{DbError, RetentionError};
use rc_vfs::VfsError;

use super::EngineError;
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
        EngineError::RetentionUnknownTable {
            table: "settings".to_owned(),
        },
        EngineError::RetentionOutOfRange {
            key: "retention.error_log.max_rows",
            value: 0,
            min: 100,
            max: 50_000,
        },
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
        EngineError::SshConfigPathRefused {
            path: "/x".to_owned(),
        },
        EngineError::SshConfigKeyMaterial {
            path: "/x".to_owned(),
        },
        EngineError::SshConfigUnreadable {
            path: "/x".to_owned(),
            source: std::io::Error::other("x"),
        },
        EngineError::SshConfigHostUnknown {
            alias: "x".to_owned(),
        },
        EngineError::TcIniPathRefused {
            path: "/x".to_owned(),
        },
        EngineError::TcIniUnreadable {
            path: "/x".to_owned(),
            source: std::io::Error::other("x"),
        },
        EngineError::TcIniTooLarge {
            path: "/x".to_owned(),
            max: 1,
        },
        EngineError::TcIniNotFound,
        EngineError::RdmPathRefused {
            path: "/x".to_owned(),
        },
        EngineError::RdmUnreadable {
            path: "/x".to_owned(),
            source: std::io::Error::other("x"),
        },
        EngineError::RdmTooLarge {
            path: "/x".to_owned(),
            max: 1,
        },
        EngineError::RdmEncrypted {
            path: "/x".to_owned(),
        },
        EngineError::RdmInvalid {
            path: "/x".to_owned(),
            line: 1,
        },
        EngineError::RdmNotAnExport {
            path: "/x".to_owned(),
        },
        EngineError::InvalidSearch {
            reason: "x".to_owned(),
        },
        EngineError::Cancelled,
        EngineError::CompareNeedsFiles,
        EngineError::CompareChanged {
            name: "a.txt".to_owned(),
        },
        EngineError::CompareNotEditable {
            name: "a.txt".to_owned(),
            reason: "too_large",
        },
        EngineError::CompareBlockNotFound { block: 3 },
        EngineError::SyncDeletesUnconfirmed { count: 1 },
        EngineError::SyncActionInvalid {
            row: 1,
            action: crate::sync::SyncAction::DeleteLeft,
        },
        EngineError::SyncRowNotFound {
            path: "x".to_owned(),
        },
        EngineError::OpenFailed {
            name: "setup.exe".to_owned(),
        },
        EngineError::DriveNotFound {
            drive: "Q:".to_owned(),
        },
        EngineError::DriveNotReady {
            drive: "D:".to_owned(),
        },
        EngineError::SettingUnknown {
            key: "x.y".to_owned(),
        },
        EngineError::SettingProtected {
            key: "mcp.policy".to_owned(),
        },
        EngineError::SettingInvalid {
            key: "appearance.theme".to_owned(),
            reason: "expected one of system, light, dark".to_owned(),
        },
        EngineError::SensitivePath {
            path: "/x".to_owned(),
            reason: "x",
        },
        EngineError::EndpointChangeNeedsGui { id: 1 },
        EngineError::RenameBusy,
        EngineError::RenameNoChanges,
        EngineError::RenameNothingToUndo,
        EngineError::RenameUndoIncomplete,
        EngineError::RenameUndoMismatch {
            mismatches: Vec::new(),
        },
        EngineError::RenamePresetInvalid {
            reason: "empty name".to_owned(),
        },
        EngineError::RenamePresetNotFound {
            name: "Photos".to_owned(),
        },
        EngineError::ConnectionNotSsh {
            id: crate::ConnectionId(1),
        },
        EngineError::ExecInvalid {
            reason: "empty command".to_owned(),
        },
        EngineError::SessionPrefsInvalid {
            reason: "x".to_owned(),
        },
        EngineError::PasswordNotStored {
            connection: crate::ConnectionId(1),
        },
        EngineError::FtpConnectionNotFound {
            connection: "nas".to_owned(),
        },
        EngineError::FtpConnectionAmbiguous {
            connection: "nas".to_owned(),
            count: 2,
        },
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
        EngineError::SftpConnectionNotFound {
            connection: "web".to_owned(),
        },
        EngineError::SftpConnectionAmbiguous {
            connection: "web".to_owned(),
            count: 2,
        },
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
            | EngineError::RenameBusy
            | EngineError::RenameNoChanges
            | EngineError::RenameNothingToUndo
            | EngineError::RenameUndoIncomplete
            | EngineError::RenameUndoMismatch { .. }
            | EngineError::RenamePresetInvalid { .. }
            | EngineError::RenamePresetNotFound { .. }
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
            | EngineError::SshConfigPathRefused { .. }
            | EngineError::SshConfigKeyMaterial { .. }
            | EngineError::SshConfigUnreadable { .. }
            | EngineError::SshConfigHostUnknown { .. }
            | EngineError::InvalidSearch { .. }
            | EngineError::Cancelled
            | EngineError::CompareNeedsFiles
            | EngineError::CompareChanged { .. }
            | EngineError::CompareNotEditable { .. }
            | EngineError::CompareBlockNotFound { .. }
            | EngineError::SyncDeletesUnconfirmed { .. }
            | EngineError::SyncActionInvalid { .. }
            | EngineError::SyncRowNotFound { .. }
            | EngineError::SensitivePath { .. }
            | EngineError::EndpointChangeNeedsGui { .. }
            | EngineError::ConnectionNotSsh { .. }
            | EngineError::ExecInvalid { .. }
            | EngineError::SessionPrefsInvalid { .. }
            | EngineError::PasswordNotStored { .. }
            | EngineError::FtpConnectionNotFound { .. }
            | EngineError::FtpConnectionAmbiguous { .. } => {}
            EngineError::RetentionUnknownTable { .. } | EngineError::RetentionOutOfRange { .. } => {
            }
            EngineError::OpenFailed { .. } => {}
            EngineError::DriveNotFound { .. } | EngineError::DriveNotReady { .. } => {}
            EngineError::SettingUnknown { .. }
            | EngineError::SettingProtected { .. }
            | EngineError::SettingInvalid { .. }
            | EngineError::McpQuestionNotFound { .. }
            | EngineError::McpAnswerNotOffered { .. } => {}
            EngineError::CommandLineInvalid { .. }
            | EngineError::CommandLineLocalOnly
            | EngineError::CommandLineFailed { .. }
            | EngineError::FilterInvalid { .. } => {}
            EngineError::ArchiveReadOnly | EngineError::NotAnArchive { .. } => {}
            EngineError::PropertyInvalid { .. } => {}
            EngineError::SftpConnectionNotFound { .. }
            | EngineError::SftpConnectionAmbiguous { .. } => {}
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
            EngineError::TcIniPathRefused { .. }
            | EngineError::TcIniUnreadable { .. }
            | EngineError::TcIniTooLarge { .. }
            | EngineError::TcIniNotFound => {}
            EngineError::RdmPathRefused { .. }
            | EngineError::RdmUnreadable { .. }
            | EngineError::RdmTooLarge { .. }
            | EngineError::RdmEncrypted { .. }
            | EngineError::RdmInvalid { .. }
            | EngineError::RdmNotAnExport { .. } => {}
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
