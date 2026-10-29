//! Private keys taken into the vault: the round trip from the key file to an opened key, and
//! what is shown of them.

use rc_vault::ExposeSecret as _;

use super::tests::{FIXTURES, MASTER, connection, engine_in, master};
use crate::test_support;
use crate::{
    AuthMethod, Command, Connection, ConnectionId, Engine, Outcome, SecretKind, SecretParam,
    SecretRef, StoredKey,
};

const ED25519_FINGERPRINT: &str = "SHA256:VqHchmOpCuMbpd8yWqFHO6llI9815J85wGd1QUaKRig";
const RSA_FINGERPRINT: &str = "SHA256:6KajqcW1Orb7TSswcvfa2oFhUNaLC46DsrFV+00r028";
const RSA_PASSPHRASE: &str = "fixture-passphrase";

fn fixture(name: &str) -> String {
    format!("{FIXTURES}/{name}")
}

fn import(id: ConnectionId, name: &str, passphrase: Option<&str>) -> Command {
    Command::ImportKey(crate::ImportKeyRequest {
        connection_id: id,
        path: fixture(name),
        passphrase: passphrase.map(SecretParam::from),
    })
}

async fn take_in(engine: &Engine, command: Command) -> Connection {
    match engine.dispatch(command).await {
        Ok(Outcome::Connection { connection }) => connection,
        other => panic!("the connection, got {other:?}"),
    }
}

async fn resolve(engine: &Engine, id: ConnectionId, kind: SecretKind) -> Option<String> {
    engine
        .resolve_secret(SecretRef {
            connection: id,
            kind,
        })
        .await
        .expect("resolves")
        .map(|secret| secret.expose_secret().to_owned())
}

/// The lines of a key file that are key material: everything but the armour and headers.
fn material(name: &str) -> Vec<String> {
    std::fs::read_to_string(fixture(name))
        .expect("the fixture")
        .lines()
        .filter(|line| !line.starts_with("-----") && !line.contains(": ") && line.len() >= 16)
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn an_ed25519_key_and_an_encrypted_rsa_key_round_trip() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let ed = connection(&engine, "ed", AuthMethod::Key).await;
    let rsa = connection(&engine, "rsa", AuthMethod::Key).await;

    // The first secret needs a master password, before the key file is even read.
    let absent = engine
        .dispatch(import(ed, "client_key_ed25519", None))
        .await;
    assert_eq!(absent.expect_err("refused").code(), "vault_absent");
    engine
        .dispatch(Command::CreateVault {
            password: master(MASTER),
        })
        .await
        .expect("create");

    let stored = take_in(&engine, import(ed, "client_key_ed25519", None)).await;
    let public = std::fs::read_to_string(fixture("client_key_ed25519.pub")).expect("public");
    assert_eq!(
        stored.key,
        Some(StoredKey {
            key_type: "ssh-ed25519".into(),
            fingerprint: ED25519_FINGERPRINT.into(),
            comment: Some("fixture@rcommander".into()),
            stored: true,
            public_key: public
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" "),
        })
    );
    assert!(stored.secrets.has_key && !stored.secrets.has_passphrase);
    let text = resolve(&engine, ed, SecretKind::Key).await.expect("stored");
    assert_eq!(
        text,
        std::fs::read_to_string(fixture("client_key_ed25519")).expect("the file")
    );
    let opened = rc_remote::private_key::decode(&text, None).expect("opens");
    assert_eq!(
        rc_remote::OfferedKey::from_public_key(opened.public_key())
            .expect("a public key")
            .fingerprint,
        ED25519_FINGERPRINT
    );

    for (passphrase, code) in [
        (None, "key_passphrase_needed"),
        (Some("not it"), "key_passphrase_wrong"),
    ] {
        let refused = engine
            .dispatch(import(rsa, "client_key_rsapem", passphrase))
            .await;
        assert_eq!(refused.expect_err("refused").code(), code);
    }
    let stored = take_in(
        &engine,
        import(rsa, "client_key_rsapem", Some(RSA_PASSPHRASE)),
    )
    .await;
    let key = stored.key.as_ref().expect("a stored key");
    assert_eq!(key.key_type, "ssh-rsa");
    assert_eq!(key.fingerprint, RSA_FINGERPRINT);
    assert!(stored.secrets.has_key && stored.secrets.has_passphrase);
    let text = resolve(&engine, rsa, SecretKind::Key)
        .await
        .expect("stored");
    let passphrase = resolve(&engine, rsa, SecretKind::Passphrase)
        .await
        .expect("stored");
    assert_eq!(passphrase, RSA_PASSPHRASE);
    let opened = rc_remote::private_key::decode(&text, Some(&passphrase)).expect("opens");
    assert_eq!(
        rc_remote::OfferedKey::from_public_key(opened.public_key())
            .expect("a public key")
            .fingerprint,
        RSA_FINGERPRINT
    );

    // What is shown of the connections carries no key material.
    let shown = format!(
        "{stored:?} {}",
        serde_json::to_string(&stored).expect("json")
    );
    for line in material("client_key_rsapem")
        .into_iter()
        .chain(material("client_key_ed25519"))
    {
        assert!(!shown.contains(&line), "{shown}");
    }
    assert!(!shown.contains(RSA_PASSPHRASE), "{shown}");
    let json = serde_json::to_value(&stored).expect("json");
    assert_eq!(
        json["key"],
        serde_json::json!({
            "type": "ssh-rsa",
            "fingerprint": RSA_FINGERPRINT,
            "comment": null,
            "stored": true,
        })
    );
}

#[tokio::test]
async fn putty_keys_are_taken_in_and_anything_else_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let web = connection(&engine, "web", AuthMethod::Key).await;
    engine
        .dispatch(Command::CreateVault {
            password: master(MASTER),
        })
        .await
        .expect("create");

    let v3 = take_in(&engine, import(web, "client_key_ed25519ppk", None)).await;
    assert_eq!(
        v3.key.as_ref().map(|key| key.key_type.as_str()),
        Some("ssh-ed25519")
    );
    let v2 = take_in(&engine, import(web, "client_key_rsappk2", Some("123"))).await;
    assert_eq!(
        v2.key.as_ref().map(|key| key.key_type.as_str()),
        Some("ssh-rsa")
    );

    let missing = Command::ImportKey(crate::ImportKeyRequest {
        connection_id: web,
        path: dir.path().join("missing").to_string_lossy().into_owned(),
        passphrase: None,
    });
    for (command, code) in [
        (
            import(web, "client_key_ed25519.pub", None),
            "key_format_unknown",
        ),
        (
            import(web, "known_hosts/known_hosts", None),
            "key_format_unknown",
        ),
        (missing, "key_file_unreadable"),
        (
            test_support::store_secret(
                web,
                SecretKind::Key,
                SecretParam::from("-----BEGIN OPENSSH PRIVATE KEY-----"),
            ),
            "key_import_only",
        ),
    ] {
        let refused = engine.dispatch(command).await;
        assert_eq!(refused.expect_err("refused").code(), code);
    }
    // The refusals left the stored key as it was.
    let kept = engine
        .dispatch(Command::GetConnection { id: web })
        .await
        .expect("read");
    let Outcome::Connection { connection } = kept else {
        panic!("a connection");
    };
    assert_eq!(connection.key, v2.key);

    // Clearing the key takes its public half with it.
    let cleared = take_in(&engine, test_support::clear_secret(web, SecretKind::Key)).await;
    assert_eq!(cleared.key, None);
    assert!(!cleared.secrets.has_key);
}

/// Only a regular file within the limit is read: a folder or a device is refused before it is
/// opened, a file one byte too large after at most that byte more.
#[tokio::test]
async fn only_a_regular_file_within_the_limit_is_read() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = |name: &str| dir.path().join(name).to_string_lossy().into_owned();
    let folder = super::read_key_file(&path("")).await.expect_err("a folder");
    assert_eq!(folder.code(), "key_file_unreadable");
    std::fs::write(dir.path().join("big"), vec![b'a'; crate::MAX_KEY_BYTES + 1]).expect("big");
    let big = super::read_key_file(&path("big"))
        .await
        .expect_err("too large");
    assert_eq!(big.code(), "key_format_unknown");
    std::fs::write(dir.path().join("fits"), vec![b'a'; crate::MAX_KEY_BYTES]).expect("fits");
    assert!(super::read_key_file(&path("fits")).await.is_ok());
    #[cfg(unix)]
    {
        let device = super::read_key_file("/dev/zero")
            .await
            .expect_err("a device");
        assert_eq!(device.code(), "key_file_unreadable");
    }
}

/// A key path a connection names is read only when it is local: for a share, a device path or
/// a URL the reader is never called (no SMB touch), for a local path it is (RC-050-06).
#[tokio::test]
async fn a_connections_key_path_off_this_computer_is_never_read() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let reads = AtomicUsize::new(0);
    let probe = |_: String| {
        reads.fetch_add(1, Ordering::SeqCst);
        async { Ok(rc_vault::SecretString::from("key")) }
    };
    for path in [
        r"\\server\share\k",
        "//server/share/k",
        r"\\?\UNC\server\share\k",
        "sftp://server/k",
        "id_rsa",
    ] {
        let refused = super::local_key_file(path, probe)
            .await
            .expect_err("refused");
        assert_eq!(refused.code(), "key_path_refused", "{path}");
    }
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    super::local_key_file("/home/ada/.ssh/id_ed25519", probe)
        .await
        .expect("read");
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}
