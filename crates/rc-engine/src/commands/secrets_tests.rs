use rc_vault::{ExposeSecret as _, VaultStateKind};
use tokio::sync::broadcast::Receiver;

use crate::test_support;
use crate::{
    AuthMethod, Command, ConnectionDraft, ConnectionId, Engine, EngineEvent, Outcome, SecretKind,
    SecretParam, SecretRef,
};

pub(super) const MASTER: &str = "correct horse battery";
const PASSWORD: &str = "hunter2-web";
/// `rc-remote`'s throw-away test keys.
pub(super) const FIXTURES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../rc-remote/tests/fixtures");

pub(super) use crate::test_support::engine_in;

pub(super) async fn connection(
    engine: &Engine,
    name: &str,
    auth_method: AuthMethod,
) -> ConnectionId {
    let draft = ConnectionDraft {
        auth_method,
        ..ConnectionDraft::new(name, "example.org")
    };
    match engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
        .expect("connection")
    {
        Outcome::Connection { connection } => connection.id,
        other => panic!("a connection, got {other:?}"),
    }
}

async fn state(engine: &Engine) -> VaultStateKind {
    match engine.dispatch(Command::GetVaultStatus).await {
        Ok(Outcome::Vault(status)) => status.state,
        other => panic!("a vault status, got {other:?}"),
    }
}

fn drain(events: &mut Receiver<EngineEvent>) -> Vec<EngineEvent> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

fn set(id: ConnectionId, kind: SecretKind, value: &str) -> Command {
    test_support::store_secret(id, kind, SecretParam::from(value))
}

pub(super) fn master(password: &str) -> SecretParam {
    SecretParam::from(password)
}

/// What a step of the lifecycle leaves: the vault's state, or the refusal's code.
#[derive(Debug)]
enum Then {
    State(VaultStateKind),
    Refused(&'static str),
}

/// The lifecycle of the master password, as a table: each command, then what it leaves.
#[tokio::test]
async fn the_master_password_lifecycle_as_a_state_table() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let web = connection(&engine, "web", AuthMethod::Password).await;
    use Then::{Refused, State};
    use VaultStateKind::{Absent, Locked, Unlocked};
    let table = [
        (Command::GetVaultStatus, State(Absent)),
        (
            Command::UnlockVault {
                password: master(MASTER),
            },
            Refused("vault_absent"),
        ),
        // The first secret: no vault yet, so the GUI asks for a new master password first.
        (
            set(web, SecretKind::Password, PASSWORD),
            Refused("vault_absent"),
        ),
        (
            Command::CreateVault {
                password: master("short"),
            },
            Refused("master_password_too_short"),
        ),
        (
            Command::CreateVault {
                password: master(MASTER),
            },
            State(Unlocked),
        ),
        (
            Command::CreateVault {
                password: master(MASTER),
            },
            Refused("vault_exists"),
        ),
        (set(web, SecretKind::Password, PASSWORD), State(Unlocked)),
        (
            set(web, SecretKind::Password, "  "),
            Refused("secret_empty"),
        ),
        (Command::LockVault, State(Locked)),
        (
            set(web, SecretKind::Passphrase, "pp"),
            Refused("vault_locked"),
        ),
        (
            Command::UnlockVault {
                password: master("wrong password"),
            },
            Refused("vault_wrong_password"),
        ),
        (
            Command::UnlockVault {
                password: master(MASTER),
            },
            State(Unlocked),
        ),
        (Command::RemoveVault, Refused("vault_in_use")),
        (
            test_support::clear_secret(web, SecretKind::Password),
            State(Unlocked),
        ),
        (Command::RemoveVault, State(Absent)),
        (Command::RemoveVault, Refused("vault_absent")),
    ];
    for (step, (command, then)) in table.into_iter().enumerate() {
        let id = command.id();
        let result = engine.dispatch(command).await;
        match (then, result) {
            (State(expected), Ok(_)) => {
                assert_eq!(state(&engine).await, expected, "step {step}: {id}");
            }
            (Refused(code), Err(error)) => assert_eq!(error.code(), code, "step {step}: {id}"),
            (then, result) => panic!("step {step}: {id}: expected {then:?}, got {result:?}"),
        }
    }
}

#[tokio::test]
async fn without_stored_secrets_nobody_is_ever_asked() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let mut events = engine.subscribe();
    let agent = connection(&engine, "agent", AuthMethod::Agent).await;
    for kind in SecretKind::ALL {
        let secret = SecretRef {
            connection: agent,
            kind,
        };
        assert!(
            engine
                .resolve_secret(secret)
                .await
                .expect("resolves")
                .is_none()
        );
    }
    assert_eq!(state(&engine).await, VaultStateKind::Absent);
    let db = engine.db().expect("db");
    assert_eq!(db.vault_meta().await.expect("read"), None);
    assert!(
        drain(&mut events)
            .iter()
            .all(|event| matches!(event, EngineEvent::ConnectionTree { .. })),
        "only the new connection is announced"
    );
}

#[tokio::test]
async fn after_a_restart_only_a_connection_that_needs_its_secret_asks() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (web, agent) = {
        let engine = engine_in(dir.path()).await;
        let web = connection(&engine, "web", AuthMethod::Password).await;
        let agent = connection(&engine, "agent", AuthMethod::Agent).await;
        engine
            .dispatch(Command::CreateVault {
                password: master(MASTER),
            })
            .await
            .expect("create");
        engine
            .dispatch(set(web, SecretKind::Password, PASSWORD))
            .await
            .expect("store");
        (web, agent)
    };

    let engine = engine_in(dir.path()).await;
    assert_eq!(state(&engine).await, VaultStateKind::Locked);
    let mut events = engine.subscribe();
    let none = engine
        .resolve_secret(SecretRef {
            connection: agent,
            kind: SecretKind::Password,
        })
        .await
        .expect("resolves");
    assert!(none.is_none());
    assert!(
        drain(&mut events).is_empty(),
        "nothing stored, nobody asked"
    );

    let needed = SecretRef {
        connection: web,
        kind: SecretKind::Password,
    };
    let refused = engine.resolve_secret(needed).await.expect_err("locked");
    assert_eq!(refused.code(), "vault_locked");
    assert_eq!(
        drain(&mut events),
        [EngineEvent::SecretNeeded { secret: needed }]
    );

    engine
        .dispatch(Command::UnlockVault {
            password: master(MASTER),
        })
        .await
        .expect("unlock");
    assert_eq!(drain(&mut events), [EngineEvent::VaultUnlocked]);
    let secret = engine
        .resolve_secret(needed)
        .await
        .expect("resolves")
        .expect("stored");
    assert_eq!(secret.expose_secret(), PASSWORD);
}

#[tokio::test]
async fn a_stored_secret_is_flagged_and_encrypted_in_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let web = connection(&engine, "web", AuthMethod::Password).await;
    engine
        .dispatch(Command::CreateVault {
            password: master(MASTER),
        })
        .await
        .expect("create");
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(set(web, SecretKind::Password, PASSWORD))
        .await
    else {
        panic!("the connection");
    };
    assert!(connection.secrets.has_password);
    assert!(!connection.secrets.has_key);
    let cipher = engine
        .db()
        .expect("db")
        .connection_secret(web, SecretKind::Password)
        .await
        .expect("read")
        .expect("stored");
    assert!(
        !cipher
            .windows(PASSWORD.len())
            .any(|window| window == PASSWORD.as_bytes())
    );
}

#[tokio::test]
async fn a_new_master_password_keeps_every_secret_readable() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let web = connection(&engine, "web", AuthMethod::Key).await;
    engine
        .dispatch(Command::CreateVault {
            password: master(MASTER),
        })
        .await
        .expect("create");
    let key_file = format!("{FIXTURES}/client_key_ed25519");
    engine
        .dispatch(Command::ImportKey(crate::ImportKeyRequest {
            connection_id: web,
            path: key_file.clone(),
            passphrase: None,
        }))
        .await
        .expect("take the key in");
    let change = |current: &str, new: &str| Command::ChangeMasterPassword {
        current: master(current),
        new: master(new),
    };
    let too_short = engine.dispatch(change(MASTER, "short")).await;
    assert_eq!(
        too_short.expect_err("refused").code(),
        "master_password_too_short"
    );
    engine
        .dispatch(change(MASTER, "a much better passphrase"))
        .await
        .expect("change");
    engine.dispatch(Command::LockVault).await.expect("lock");
    let old = engine
        .dispatch(Command::UnlockVault {
            password: master(MASTER),
        })
        .await;
    assert_eq!(old.expect_err("refused").code(), "vault_wrong_password");
    engine
        .dispatch(Command::UnlockVault {
            password: master("a much better passphrase"),
        })
        .await
        .expect("unlock");
    let key = engine
        .resolve_secret(SecretRef {
            connection: web,
            kind: SecretKind::Key,
        })
        .await
        .expect("resolves")
        .expect("stored");
    let file = std::fs::read_to_string(&key_file).expect("the fixture");
    assert_eq!(key.expose_secret(), file);
}

#[tokio::test]
async fn deleting_the_last_secret_offers_to_remove_the_vault() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let web = connection(&engine, "web", AuthMethod::Password).await;
    let db_box = connection(&engine, "db", AuthMethod::Password).await;
    engine
        .dispatch(Command::CreateVault {
            password: master(MASTER),
        })
        .await
        .expect("create");
    for id in [web, db_box] {
        engine
            .dispatch(set(id, SecretKind::Password, PASSWORD))
            .await
            .expect("store");
    }
    let mut events = engine.subscribe();
    engine
        .dispatch(test_support::clear_secret(web, SecretKind::Password))
        .await
        .expect("clear");
    assert!(!drain(&mut events).contains(&EngineEvent::VaultUnused));
    engine
        .dispatch(Command::DeleteConnection { id: db_box })
        .await
        .expect("delete");
    assert!(drain(&mut events).contains(&EngineEvent::VaultUnused));

    engine.dispatch(Command::RemoveVault).await.expect("remove");
    assert_eq!(drain(&mut events), [EngineEvent::VaultRemoved]);
    assert_eq!(state(&engine).await, VaultStateKind::Absent);
}

#[tokio::test]
async fn mcp_reaches_none_of_the_secret_commands() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let web = connection(&engine, "web", AuthMethod::Password).await;
    for command in [
        Command::CreateVault {
            password: master(MASTER),
        },
        Command::UnlockVault {
            password: master(MASTER),
        },
        Command::ChangeMasterPassword {
            current: master(MASTER),
            new: master(MASTER),
        },
        Command::RemoveVault,
        set(web, SecretKind::Password, PASSWORD),
        Command::ImportKey(crate::ImportKeyRequest {
            connection_id: web,
            path: "id_ed25519".into(),
            passphrase: None,
        }),
        test_support::clear_secret(web, SecretKind::Password),
    ] {
        let id = command.id();
        let refused = engine.dispatch_mcp(command).await.expect_err(id);
        assert_eq!(refused.code(), "gui_only", "{id}");
    }
    assert_eq!(state(&engine).await, VaultStateKind::Absent);
}
