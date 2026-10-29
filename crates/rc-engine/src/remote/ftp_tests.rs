use rc_remote::ftp::{Credentials, DataMode, Encryption, NameEncoding};
use rc_vault::{ExposeSecret as _, SecretString};

use super::*;
use crate::{ConnectionDraft, DefaultView, SecretFlags};

fn connection(kind: ConnectionKind, auth_method: AuthMethod) -> Connection {
    let draft = ConnectionDraft::new("nas", "nas.local");
    Connection {
        id: ConnectionId(7),
        folder_id: None,
        name: draft.name,
        kind,
        host: draft.host,
        port: 2121,
        username: Some("web".into()),
        auth_method,
        key_path: None,
        login_script_id: None,
        default_view: DefaultView::Files,
        start_directory: None,
        notes: None,
        ftp_data_mode: None,
        ftp_encoding: None,
        sort_order: 0,
        created_at: String::new(),
        updated_at: String::new(),
        secrets: SecretFlags::default(),
        key: None,
        credential_id: None,
    }
}

#[test]
fn the_kind_is_the_encryption() {
    assert_eq!(encryption(ConnectionKind::Ftp), Some(Encryption::None));
    assert_eq!(encryption(ConnectionKind::Ftps), Some(Encryption::Explicit));
    assert_eq!(
        encryption(ConnectionKind::FtpsImplicit),
        Some(Encryption::Implicit)
    );
    assert_eq!(encryption(ConnectionKind::Ssh), None);
}

#[test]
fn a_password_login_takes_the_user_and_the_stored_password() {
    let config = ftp_config(
        &connection(ConnectionKind::Ftps, AuthMethod::Password),
        Some(SecretString::from("hunter2-ftp")),
    )
    .expect("config");
    assert_eq!(config.host, "nas.local");
    assert_eq!(config.port, 2121, "the connection's port, not the default");
    assert_eq!(config.encryption, Encryption::Explicit);
    let Credentials::User { name, password } = &config.credentials else {
        panic!("a user login, got {:?}", config.credentials);
    };
    assert_eq!(name, "web");
    assert_eq!(password.expose_secret(), "hunter2-ftp");
    let shown = format!("{config:?}");
    assert!(!shown.contains("hunter2-ftp"), "{shown}");
}

#[test]
fn an_anonymous_login_needs_no_password() {
    let config = ftp_config(
        &connection(ConnectionKind::Ftp, AuthMethod::Anonymous),
        None,
    )
    .expect("config");
    assert!(matches!(config.credentials, Credentials::Anonymous));
    assert_eq!(config.encryption, Encryption::None);
}

#[test]
fn a_password_login_without_a_password_is_refused() {
    let error = ftp_config(
        &connection(ConnectionKind::FtpsImplicit, AuthMethod::Password),
        None,
    )
    .expect_err("no password");
    assert_eq!(error.code(), "password_not_stored");
}

#[test]
fn an_ssh_connection_is_no_ftp_connection() {
    let error = ftp_config(
        &connection(ConnectionKind::Ssh, AuthMethod::Password),
        Some(SecretString::from("x")),
    )
    .expect_err("ssh");
    assert_eq!(error.code(), "ftp_connection_not_found");
}

#[test]
fn the_ftp_options_reach_the_configuration() {
    let mut active = connection(ConnectionKind::Ftp, AuthMethod::Anonymous);
    let defaults = ftp_config(&active, None).expect("config");
    assert_eq!(defaults.data_mode, DataMode::Passive);
    assert_eq!(defaults.encoding, NameEncoding::Latin1);
    active.ftp_data_mode = Some(FtpDataMode::Active);
    active.ftp_encoding = Some(FtpEncoding::Utf8);
    let config = ftp_config(&active, None).expect("config");
    assert_eq!(config.data_mode, DataMode::Active);
    assert_eq!(config.encoding, NameEncoding::Utf8);
}
