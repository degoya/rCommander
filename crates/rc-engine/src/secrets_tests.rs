use std::time::Duration;

use rc_db::{Db, SECRET_COLUMNS};
use rc_vault::{ManualClock, Vault};

use super::*;
use crate::test_support;
use crate::{AuthMethod, Command, ConnectionDraft, Outcome};

const KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\n\
QyNTUxOQAAACBkZXZlbG9wZXIta2V5LWZvci10ZXN0cy1vbmx5AAAAAAAAAAAAAAAA==\n\
-----END OPENSSH PRIVATE KEY-----\n";

/// A throw-away ed25519 key without a passphrase (`rc-remote`'s test fixtures).
const ED25519_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-remote/tests/fixtures/client_key_ed25519"
);

/// An engine on an in-memory database with an unlocked vault and connection 1.
async fn unlocked() -> (tempfile::TempDir, Engine, ConnectionId) {
    unlocked_on(&ManualClock::new()).await
}

/// [`unlocked`] with the vault's time from `clock`.
async fn unlocked_on(clock: &ManualClock) -> (tempfile::TempDir, Engine, ConnectionId) {
    let root = tempfile::tempdir().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    let engine = test_support::headless(root.path()).await;
    engine.attach_db(db.clone());
    let vault = Vault::open_with(db, test_support::fast_vault_options(clock, SECRET_COLUMNS))
        .await
        .expect("vault");
    engine.attach_vault(vault);
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from("correct horse battery"),
        })
        .await
        .expect("create");
    let draft = ConnectionDraft {
        auth_method: AuthMethod::Key,
        ..ConnectionDraft::new("web", "example.org")
    };
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
    else {
        panic!("a connection");
    };
    (root, engine, connection.id)
}

#[test]
fn a_secret_parameter_hides_its_value() {
    let secret = SecretParam::new("hunter2-canary".to_owned());
    assert_eq!(format!("{secret:?}"), "SecretParam([redacted])");
    assert_eq!(
        serde_json::to_value(&secret).expect("serializes"),
        serde_json::json!(REDACTED)
    );
    let read: SecretParam = serde_json::from_str("\"hunter2-canary\"").expect("deserializes");
    assert_eq!(read, secret);
    assert!(!secret.is_empty());
    assert!(SecretParam::from("").is_empty());
}

#[test]
fn a_master_password_has_a_minimum_length() {
    let short = SecretParam::from("1234567");
    assert_eq!(
        short.master_password().map(|_| ()).map_err(|e| e.code()),
        Err("master_password_too_short")
    );
    // Characters, not bytes: eight umlauts are long enough.
    let wide = SecretParam::from("\u{e4}\u{e4}\u{e4}\u{e4}\u{e4}\u{e4}\u{e4}\u{e4}");
    assert!(wide.master_password().is_ok());
}

#[test]
fn a_key_is_scrubbed_whole_and_line_by_line_but_not_its_armour() {
    let pieces: Vec<&str> = fragments(KEY).collect();
    assert_eq!(pieces[0], KEY);
    assert_eq!(pieces.len(), 3, "{pieces:?}");
    assert!(pieces[1..].iter().all(|line| !line.starts_with("-----")));
    assert_eq!(fragments("hunter2").count(), 1);
}

/// RC-080-11 (V3): what was stored or read is scrubbed while the vault is unlocked and after it
/// locked - a password still on an SSH screen does not leave through `terminal_read_screen` - and
/// only its fingerprint is kept, never its value.
#[tokio::test]
async fn known_secrets_stay_scrubbed_after_the_vault_locks() {
    let (_root, engine, web) = unlocked().await;
    engine
        .dispatch(test_support::store_secret(
            web,
            SecretKind::Password,
            SecretParam::from("hunter2-canary"),
        ))
        .await
        .expect("store");
    engine
        .dispatch(Command::ImportKey(crate::ImportKeyRequest {
            connection_id: web,
            path: ED25519_FILE.to_owned(),
            passphrase: None,
        }))
        .await
        .expect("take the key in");
    assert_eq!(engine.scrub_secrets("nothing here"), None);
    assert_eq!(
        engine
            .scrub_secrets("login hunter2-canary ok")
            .expect("scrubbed"),
        format!("login {REDACTED} ok")
    );
    let key = std::fs::read_to_string(ED25519_FILE).expect("the fixture");
    let screen = key.replace('\n', "\r\n");
    let scrubbed = engine.scrub_secrets(&screen).expect("scrubbed");
    for line in key.lines().filter(|line| !line.starts_with("-----")) {
        assert!(!scrubbed.contains(line), "{scrubbed}");
    }
    assert!(scrubbed.contains("-----BEGIN OPENSSH PRIVATE KEY-----"));

    engine.dispatch(Command::LockVault).await.expect("lock");
    let screen = "$ ssh web\r\nPassword: hunter2-canary\r\nLast login: today\r\n";
    assert_eq!(
        engine.scrub_secrets(screen).expect("still scrubbed"),
        format!("$ ssh web\r\nPassword: {REDACTED}\r\nLast login: today\r\n")
    );
    let scrubbed = engine
        .scrub_secrets(&key.replace('\n', "\r\n"))
        .expect("the key too");
    for line in key.lines().filter(|line| !line.starts_with("-----")) {
        assert!(!scrubbed.contains(line), "{scrubbed}");
    }
    // After a restart of the vault nothing is forgotten either.
    engine
        .dispatch(Command::UnlockVault {
            password: SecretParam::from("correct horse battery"),
        })
        .await
        .expect("unlock");
    assert!(engine.scrub_secrets("hunter2-canary").is_some());
}

/// RC-080-11 (V1): a secret an agent's call reads over MCP opens, but does not defer the idle
/// lock; the window's own read does.
#[tokio::test]
async fn mcp_reads_do_not_defer_the_idle_lock() {
    let clock = ManualClock::new();
    let (_root, engine, web) = unlocked_on(&clock).await;
    let vault = engine.vault().expect("a vault");
    vault.set_idle_lock_minutes(15).await.expect("setting");
    engine
        .dispatch(test_support::store_secret(
            web,
            SecretKind::Password,
            SecretParam::from("hunter2-canary"),
        ))
        .await
        .expect("store");
    let secret = SecretRef {
        connection: web,
        kind: SecretKind::Password,
    };
    for _ in 0..2 {
        clock.advance(Duration::from_secs(7 * 60));
        crate::read_guard::as_mcp(engine.resolve_secret(secret))
            .await
            .expect("the agent's read opens")
            .expect("stored");
    }
    clock.advance(Duration::from_secs(60));
    let refused = crate::read_guard::as_mcp(engine.resolve_secret(secret))
        .await
        .expect_err("fifteen minutes since the person's last use");
    assert_eq!(refused.code(), "vault_locked");

    engine
        .dispatch(Command::UnlockVault {
            password: SecretParam::from("correct horse battery"),
        })
        .await
        .expect("unlock");
    clock.advance(Duration::from_secs(14 * 60));
    engine
        .resolve_secret(secret)
        .await
        .expect("the window's read")
        .expect("stored");
    clock.advance(Duration::from_secs(14 * 60));
    assert!(
        engine.resolve_secret(secret).await.is_ok(),
        "the window's read deferred the lock"
    );
}

#[tokio::test]
async fn short_secrets_are_not_remembered() {
    let (_root, engine, web) = unlocked().await;
    engine
        .dispatch(test_support::store_secret(
            web,
            SecretKind::Passphrase,
            SecretParam::from("abc"),
        ))
        .await
        .expect("store");
    assert_eq!(engine.scrub_secrets("abc abc"), None);
}

#[tokio::test]
async fn a_value_moved_to_another_row_does_not_decrypt() {
    let (_root, engine, web) = unlocked().await;
    engine
        .dispatch(test_support::store_secret(
            web,
            SecretKind::Password,
            SecretParam::from("hunter2-canary"),
        ))
        .await
        .expect("store");
    let db = engine.db().expect("db").clone();
    let cipher = db
        .connection_secret(web, SecretKind::Password)
        .await
        .expect("read")
        .expect("stored");
    db.set_connection_secret(web, SecretKind::Passphrase, Some(cipher.as_slice()))
        .await
        .expect("copy");
    let moved = engine
        .resolve_secret(SecretRef {
            connection: web,
            kind: SecretKind::Passphrase,
        })
        .await
        .expect_err("refused");
    assert_eq!(moved.code(), "vault_undecryptable");
}

/// A locked vault is announced to the window (`SecretNeeded`, the unlock dialog) for its own
/// calls only: in an MCP dispatch `resolve_secret` answers `vault_locked` and announces nothing,
/// whatever path asked for the secret.
#[tokio::test]
async fn resolve_secret_never_announces_in_an_mcp_dispatch() {
    let (_root, engine, web) = unlocked().await;
    engine
        .dispatch(test_support::store_secret(
            web,
            SecretKind::Password,
            SecretParam::from("hunter2-canary"),
        ))
        .await
        .expect("store");
    engine.dispatch(Command::LockVault).await.expect("lock");
    let secret = SecretRef {
        connection: web,
        kind: SecretKind::Password,
    };
    let mut events = engine.subscribe();
    let refused = crate::read_guard::as_mcp(engine.resolve_secret(secret))
        .await
        .expect_err("locked");
    assert_eq!(refused.code(), "vault_locked");
    assert!(
        !std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, EngineEvent::SecretNeeded { .. })),
        "no unlock dialog for an agent"
    );
    let refused = engine.resolve_secret(secret).await.expect_err("locked");
    assert_eq!(refused.code(), "vault_locked");
    assert!(
        std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, EngineEvent::SecretNeeded { .. })),
        "the window's own call asks for the master password"
    );
}
