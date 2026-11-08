//! Wrapped failures keep their own codes and details.

use super::*;

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
