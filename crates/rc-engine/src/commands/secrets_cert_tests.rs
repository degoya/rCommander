//! OpenSSH user certificates taken in with their key (RC-060-09): the `<key>-cert.pub` beside
//! the key file, what is stored and shown of it, and what a login is handed.

use std::path::Path;

use rc_remote::ssh::Credentials as _;
use serde_json::json;

use super::tests::{FIXTURES, MASTER, connection, engine_in, master};
use crate::{
    AuthMethod, Command, Connection, ConnectionId, Engine, Outcome, SecretParam,
    sessions::EngineCredentials, settings::SSH_CERTIFICATES_KEY,
};

/// `client_key_ed25519` copied into `dir` as `id_ed25519`, with `certificate` (a fixture of
/// `rc-remote/tests/fixtures/certs`) beside it as `id_ed25519-cert.pub`; the key's path.
fn key_with(dir: &Path, certificate: Option<&str>) -> String {
    let key = dir.join("id_ed25519");
    std::fs::copy(format!("{FIXTURES}/client_key_ed25519"), &key).expect("copy the key");
    if let Some(name) = certificate {
        std::fs::copy(
            format!("{FIXTURES}/certs/{name}"),
            dir.join("id_ed25519-cert.pub"),
        )
        .expect("copy the certificate");
    }
    key.to_string_lossy().into_owned()
}

async fn ready(dir: &Path) -> (Engine, ConnectionId) {
    let engine = engine_in(dir).await;
    let id = connection(&engine, "ca", AuthMethod::Key).await;
    engine
        .dispatch(Command::CreateVault {
            password: master(MASTER),
        })
        .await
        .expect("create");
    (engine, id)
}

fn import(id: ConnectionId, path: &str) -> Command {
    Command::ImportKey(crate::ImportKeyRequest {
        connection_id: id,
        path: path.to_owned(),
        passphrase: None::<SecretParam>,
    })
}

async fn take_in(engine: &Engine, command: Command) -> Connection {
    match engine.dispatch(command).await {
        Ok(Outcome::Connection { connection }) => connection,
        other => panic!("the connection, got {other:?}"),
    }
}

fn credentials(engine: &Engine, id: ConnectionId, key_path: Option<String>) -> EngineCredentials {
    EngineCredentials {
        engine: engine.clone(),
        connection: id,
        key_path,
        announce: true,
    }
}

#[tokio::test]
async fn the_certificate_beside_a_key_is_taken_in_with_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let keys = tempfile::tempdir().expect("key dir");
    let (engine, id) = ready(dir.path()).await;
    let path = key_with(keys.path(), Some("client_ed25519-cert.pub"));

    let stored = take_in(&engine, import(id, &path)).await;
    let certificate = stored
        .key
        .as_ref()
        .and_then(|key| key.certificate.as_ref())
        .expect("a certificate");
    assert_eq!(certificate.key_id, "rc-test-valid");
    assert_eq!(certificate.principals, ["tester"]);
    assert_eq!(certificate.valid_before, None);

    // MCP sees what it says, never its line.
    let shown = serde_json::to_value(&stored).expect("serializes");
    assert_eq!(
        shown["key"]["certificate"]["key_id"],
        json!("rc-test-valid")
    );
    assert!(shown["key"]["certificate"].get("openssh").is_none());

    // The login is handed the line, with the key from the vault.
    let text = std::fs::read_to_string(keys.path().join("id_ed25519-cert.pub")).expect("read");
    let material = credentials(&engine, id, None)
        .private_key()
        .await
        .expect("material")
        .expect("a key");
    assert_eq!(material.certificate.as_deref(), Some(text.trim()));
}

#[tokio::test]
async fn a_key_without_a_certificate_beside_it_stores_none() {
    let dir = tempfile::tempdir().expect("temp dir");
    let keys = tempfile::tempdir().expect("key dir");
    let (engine, id) = ready(dir.path()).await;
    let stored = take_in(&engine, import(id, &key_with(keys.path(), None))).await;
    assert_eq!(stored.key.and_then(|key| key.certificate), None);
}

#[tokio::test]
async fn another_keys_certificate_beside_it_refuses_the_import() {
    let dir = tempfile::tempdir().expect("temp dir");
    let keys = tempfile::tempdir().expect("key dir");
    let (engine, id) = ready(dir.path()).await;
    let path = key_with(keys.path(), Some("host_key_b-cert.pub"));
    let error = engine
        .dispatch(import(id, &path))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "ssh_certificate_key_mismatch");
}

#[tokio::test]
async fn an_expired_certificate_is_stored_and_shows_its_end() {
    let dir = tempfile::tempdir().expect("temp dir");
    let keys = tempfile::tempdir().expect("key dir");
    let (engine, id) = ready(dir.path()).await;
    let path = key_with(keys.path(), Some("client_ed25519-expired-cert.pub"));
    let stored = take_in(&engine, import(id, &path)).await;
    let certificate = stored
        .key
        .and_then(|key| key.certificate)
        .expect("a certificate");
    assert_eq!(
        certificate.validity_at(rc_remote::totp::unix_now()),
        rc_db::CertificateValidity::Expired
    );
}

#[tokio::test]
async fn with_certificates_off_none_is_taken_or_handed_out() {
    let dir = tempfile::tempdir().expect("temp dir");
    let keys = tempfile::tempdir().expect("key dir");
    let (engine, id) = ready(dir.path()).await;
    engine
        .set_protected_setting(SSH_CERTIFICATES_KEY, &json!(false))
        .await
        .expect("off");
    let path = key_with(keys.path(), Some("client_ed25519-cert.pub"));
    let stored = take_in(&engine, import(id, &path)).await;
    assert_eq!(stored.key.and_then(|key| key.certificate), None);

    // A key file's certificate beside it is not read either.
    let by_file = connection(&engine, "file", AuthMethod::Key).await;
    let material = credentials(&engine, by_file, Some(path))
        .private_key()
        .await
        .expect("material")
        .expect("a key");
    assert_eq!(material.certificate, None);
}

#[tokio::test]
async fn a_key_file_brings_the_certificate_beside_it_to_the_login() {
    let dir = tempfile::tempdir().expect("temp dir");
    let keys = tempfile::tempdir().expect("key dir");
    let engine = engine_in(dir.path()).await;
    let id = connection(&engine, "file", AuthMethod::Key).await;
    let path = key_with(keys.path(), Some("client_ed25519-cert.pub"));
    let material = credentials(&engine, id, Some(path))
        .private_key()
        .await
        .expect("material")
        .expect("a key");
    let certificate = material.certificate.expect("the certificate");
    assert!(
        certificate.starts_with("ssh-ed25519-cert-v01@openssh.com "),
        "{certificate}"
    );
}
