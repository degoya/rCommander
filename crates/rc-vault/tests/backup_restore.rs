//! RC-010-12 end to end: a backup, a change, a restore and a restart give back the connections,
//! the trusted host keys and the encrypted secrets - readable with the master password they were
//! stored under, and with nothing else.

use std::time::Duration;

use rc_db::{
    DB_FILE, Db, KnownHostSource, NewKnownHost, SECRETS_TABLE, SecretKind, apply_pending_restore,
    connections::ConnectionDraft,
};
use rc_vault::{
    ExposeSecret as _, FieldRef, KdfParams, MasterPassword, SecretString, Vault, VaultError,
    VaultOptions,
};

/// Parameters that derive in milliseconds; a vault always reopens with its stored ones.
fn options() -> VaultOptions {
    VaultOptions {
        kdf: KdfParams {
            m_cost_kib: 64,
            t_cost: 1,
            p_cost: 1,
        },
        failure_delay: Duration::ZERO,
        ..VaultOptions::default()
    }
}

fn password(text: &str) -> MasterPassword {
    MasterPassword::new(text.to_owned())
}

#[tokio::test]
async fn backup_restore_and_restart_keep_connections_host_keys_and_secrets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(DB_FILE);
    let db = Db::open(&path).await.expect("open");
    let connection = db
        .create_connection(None, &ConnectionDraft::new("build box", "build.example"))
        .await
        .expect("connection");
    db.add_known_host(&NewKnownHost {
        host: "build.example".to_owned(),
        port: 22,
        algorithm: "ssh-ed25519".to_owned(),
        public_key: "AAAAC3NzaC1lZDI1NTE5AAAAIExample".to_owned(),
        fingerprint_sha256: "SHA256:example".to_owned(),
        source: KnownHostSource::User,
    })
    .await
    .expect("host key");
    let vault = Vault::open_with(db.clone(), options())
        .await
        .expect("vault");
    vault
        .create(&password("correct horse"))
        .await
        .expect("create");
    // The connection's password in its real secret column (`connections.password_enc`, 0006),
    // bound to its row.
    let row = connection.id.0.to_string();
    let field = FieldRef::new(SECRETS_TABLE, SecretKind::Password.column(), &row);
    let sealed = vault
        .encrypt_str(&field, &SecretString::from("s3cret".to_owned()))
        .expect("encrypt");
    db.set_connection_secret(connection.id, SecretKind::Password, Some(&sealed))
        .await
        .expect("store");

    let backup = db
        .backup_to(dir.path().join("full.db"), false)
        .await
        .expect("backup");

    // Later: the connection is gone.
    db.delete_connection(connection.id).await.expect("delete");
    let report = db.restore_from(&backup.path).await.expect("restore");
    assert!(report.checked.has_vault);
    drop(vault);
    db.close().await;

    // The next start.
    assert!(apply_pending_restore(&path).await.expect("swap"));
    let db = Db::open(&path).await.expect("reopen");
    let restored = db.connection(connection.id).await.expect("connection back");
    assert_eq!(restored.name, "build box");
    let keys = db.known_hosts().await.expect("host keys");
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].fingerprint_sha256, "SHA256:example");

    let vault = Vault::open_with(db.clone(), options())
        .await
        .expect("vault");
    let sealed = db
        .connection_secret(connection.id, SecretKind::Password)
        .await
        .expect("read")
        .expect("stored with the restored connection");
    assert!(
        matches!(
            vault.decrypt_string(&field, &sealed),
            Err(VaultError::Locked)
        ),
        "a restored vault starts locked"
    );
    assert!(matches!(
        vault.unlock(&password("wrong")).await,
        Err(VaultError::WrongPassword)
    ));
    vault
        .unlock(&password("correct horse"))
        .await
        .expect("unlock");
    let secret = vault.decrypt_string(&field, &sealed).expect("decrypt");
    assert_eq!(secret.expose_secret(), "s3cret");
    db.close().await;
}
