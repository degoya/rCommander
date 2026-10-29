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
        EngineError::QueueListEmpty,
        EngineError::PanelChanged { panel: "left" },
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
            | EngineError::KeyFileUnreadable { .. }
            | EngineError::KeyPathRefused { .. }
            | EngineError::SshConfigPathRefused { .. }
            | EngineError::SshConfigKeyMaterial { .. }
            | EngineError::SshConfigUnreadable { .. }
            | EngineError::SshConfigHostUnknown { .. }
            | EngineError::InvalidSearch { .. }
            | EngineError::Cancelled
            | EngineError::CompareNeedsFiles
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
            EngineError::SavedQueueNotFound { .. } | EngineError::QueueListEmpty => {}
            EngineError::PanelChanged { .. } => {}
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

#[test]
fn a_file_system_failure_keeps_its_code() {
    let error = EngineError::from(VfsError::NotFound { path: "/x".into() });
    assert_eq!(error.code(), "not_found");
    assert!(!EngineError::ALL_CODES.contains(&error.code()));
}

#[test]
fn a_terminal_or_connection_failure_keeps_its_code() {
    let term = EngineError::from(TermError::Exited);
    assert_eq!(term.code(), "terminal_exited");
    assert!(TermError::ALL_CODES.contains(&term.code()));
    let connection = EngineError::from(ConnectionError::NameEmpty);
    assert_eq!(connection.code(), "name_empty");
    assert!(ConnectionError::ALL_CODES.contains(&connection.code()));
}

#[test]
fn a_retention_refusal_keeps_its_details() {
    let error = EngineError::from(RetentionError::OutOfRange {
        key: "retention.error_log.max_age_days",
        value: 0,
        min: 1,
        max: 365,
    });
    assert_eq!(error.code(), "retention_out_of_range");
    assert_eq!(
        error.to_string(),
        "retention.error_log.max_age_days must be between 1 and 365, not 0"
    );
}

#[test]
fn an_operation_failure_keeps_its_code() {
    let error = EngineError::from(rc_ops::OpsError::JobNotFound { job: 7 });
    assert_eq!(error.code(), "job_not_found");
    assert!(!EngineError::ALL_CODES.contains(&error.code()));
}

#[test]
fn an_agent_failure_keeps_its_code() {
    let error = EngineError::from(rc_remote::AgentError::NotFound {
        detail: "SSH_AUTH_SOCK is not set".to_owned(),
    });
    assert_eq!(error.code(), "agent_not_found");
    assert!(rc_remote::AgentError::ALL_CODES.contains(&error.code()));
    assert!(!EngineError::ALL_CODES.contains(&error.code()));
}

#[test]
fn a_vault_failure_keeps_its_code_and_locked_is_the_engine_s() {
    let wrong = EngineError::from(rc_vault::VaultError::WrongPassword);
    assert_eq!(wrong.code(), "vault_wrong_password");
    assert!(rc_vault::VaultError::ALL_CODES.contains(&wrong.code()));
    let locked = EngineError::from(rc_vault::VaultError::Locked);
    assert!(matches!(locked, EngineError::VaultLocked));
    assert!(locked.to_string().contains("rCommander window"), "{locked}");
}

#[test]
fn an_export_failure_keeps_its_code() {
    let error = EngineError::from(crate::exchange::ExchangeError::PasswordWrong);
    assert_eq!(error.code(), "exchange_password_wrong");
    assert!(crate::exchange::ExchangeError::ALL_CODES.contains(&error.code()));
    assert!(!EngineError::ALL_CODES.contains(&error.code()));
}

#[test]
fn a_backup_failure_keeps_its_code() {
    let error = EngineError::from(rc_db::BackupError::TargetExists {
        path: "copy.db".into(),
    });
    assert_eq!(error.code(), "backup_target_exists");
    assert!(rc_db::BackupError::ALL_CODES.contains(&error.code()));
    assert!(!EngineError::ALL_CODES.contains(&error.code()));
}

#[test]
fn an_ssh_failure_keeps_its_code_and_the_engine_s_own_cases_map_to_its_variants() {
    use rc_remote::ssh::{CredentialError, HostKeyRefusal};
    let lost = EngineError::from(rc_remote::SshError::ConnectionLost);
    assert_eq!(lost.code(), "connection_lost");
    assert!(rc_remote::SshError::ALL_CODES.contains(&lost.code()));
    assert!(!EngineError::ALL_CODES.contains(&lost.code()));
    let locked = EngineError::from(rc_remote::SshError::Credential(CredentialError::Locked));
    assert!(matches!(locked, EngineError::VaultLocked));
    assert!(matches!(
        EngineError::from(rc_remote::SshError::Cancelled),
        EngineError::Cancelled
    ));
    let unknown = EngineError::from(rc_remote::SshError::HostKey {
        host: "box".to_owned(),
        port: 22,
        offered: Box::new(rc_remote::OfferedKey {
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: "SHA256:x".to_owned(),
            public_key: "AAAA".to_owned(),
        }),
        refusal: HostKeyRefusal::Unknown,
    });
    assert!(
        matches!(&unknown, EngineError::HostKeyUnknown { fingerprint, .. } if fingerprint == "SHA256:x"),
        "{unknown:?}"
    );
    let wrong = EngineError::from(rc_remote::SshError::Key(
        rc_remote::PrivateKeyError::PassphraseWrong,
    ));
    assert!(matches!(wrong, EngineError::KeyPassphraseWrong));
}

#[test]
fn an_ftp_failure_keeps_its_code_also_inside_a_file_system_error() {
    let direct = EngineError::from(rc_remote::FtpError::LoginFailed);
    assert_eq!(direct.code(), "ftp_login_failed");
    assert!(rc_remote::FtpError::ALL_CODES.contains(&direct.code()));
    // A `Vfs` call that had to log in again carries the session failure inside `Io`.
    let reconnect = EngineError::from(VfsError::Io {
        path: "/pub".into(),
        source: std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            rc_remote::FtpError::LoginFailed,
        ),
    });
    assert_eq!(reconnect.code(), "ftp_login_failed");
    let plain = EngineError::from(VfsError::Io {
        path: "/pub".into(),
        source: std::io::Error::other("disk"),
    });
    assert_eq!(
        plain.code(),
        VfsError::Io {
            path: "/".into(),
            source: std::io::Error::other("x"),
        }
        .code()
    );
}

#[test]
fn a_missing_password_names_no_secret() {
    let error = EngineError::PasswordNotStored {
        connection: crate::ConnectionId(4),
    };
    assert_eq!(error.code(), "password_not_stored");
    assert!(error.to_string().contains("connection 4"), "{error}");
}
