//! Shared credentials (RC-050-12): one password changed once is every user's, a new master
//! password re-encrypts them with the rest, the setting turns them off, and an agent can neither
//! fill one nor point a connection at one.

use rc_vault::ExposeSecret as _;

use super::*;
use crate::{
    Command, ConnectionDraft, ConnectionPatch, CredentialDraft, CredentialPatch,
    SaveConnectionRequest, SaveFields, SecretChange, SecretKind, SecretParam, SecretRef,
    test_support::engine_in,
};

const MASTER: &str = "correct horse battery";
const SHARED: &str = "canary-shared-password";

/// An engine with an unlocked vault on a database in a new temporary directory.
async fn unlocked() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("vault created");
    (dir, engine)
}

fn set(value: &str) -> SecretChange {
    SecretChange::Set {
        kind: SecretKind::Password,
        value: SecretParam::from(value),
    }
}

/// A new entry named `name` for user `deploy`, with the password `password`.
fn create(name: &str, password: &str) -> SaveCredentialRequest {
    SaveCredentialRequest {
        fields: CredentialFields::Create {
            folder_id: None,
            credential: CredentialDraft {
                name: name.into(),
                username: Some("deploy".into()),
            },
        },
        secrets: vec![set(password)],
    }
}

async fn save(engine: &Engine, request: SaveCredentialRequest) -> Credential {
    match engine.dispatch(Command::SaveCredential(request)).await {
        Ok(Outcome::Credential { credential }) => credential,
        other => panic!("a credential: {other:?}"),
    }
}

/// A new SSH connection `name` without a user, pointed at `credential` in the same save.
async fn connection_on(engine: &Engine, name: &str, credential: CredentialId) -> ConnectionId {
    let request = SaveConnectionRequest {
        fields: SaveFields::Create {
            folder_id: None,
            connection: ConnectionDraft::new(name, "example.org"),
        },
        secrets: Vec::new(),
        login_script: None,
        favourite: None,
        credential: Some(Some(credential)),
    };
    match engine.dispatch(Command::SaveConnection(request)).await {
        Ok(Outcome::Connection { connection }) => connection.id,
        other => panic!("a connection: {other:?}"),
    }
}

async fn password(engine: &Engine, connection: ConnectionId) -> Option<String> {
    let secret = SecretRef {
        connection,
        kind: SecretKind::Password,
    };
    engine
        .resolve_secret(secret)
        .await
        .expect("resolved")
        .map(|value| value.expose_secret().to_owned())
}

#[tokio::test]
async fn two_connections_on_one_entry_use_its_password_changed_once() {
    let (_dir, engine) = unlocked().await;
    let shared = save(&engine, create("ops", "first-shared-password")).await;
    let web = connection_on(&engine, "web", shared.id).await;
    let db_host = connection_on(&engine, "db", shared.id).await;
    let changed = SaveCredentialRequest {
        fields: CredentialFields::Update {
            id: shared.id,
            patch: CredentialPatch::default(),
        },
        secrets: vec![set(SHARED)],
    };
    save(&engine, changed).await;
    for id in [web, db_host] {
        assert_eq!(password(&engine, id).await.as_deref(), Some(SHARED));
        let login = engine.login_connection(id).await.expect("login");
        assert_eq!(
            login.username.as_deref(),
            Some("deploy"),
            "the entry's user"
        );
    }
}

#[tokio::test]
async fn a_user_of_its_own_wins_and_the_setting_off_uses_its_own_secrets() {
    let (_dir, engine) = unlocked().await;
    let shared = save(&engine, create("ops", SHARED)).await;
    let web = connection_on(&engine, "web", shared.id).await;
    let own_user = SaveConnectionRequest {
        fields: SaveFields::Update {
            id: web,
            patch: ConnectionPatch {
                username: Some("root".into()),
                ..ConnectionPatch::default()
            },
        },
        secrets: vec![set("own-password-of-web")],
        login_script: None,
        favourite: None,
        credential: None,
    };
    engine
        .dispatch(Command::SaveConnection(own_user))
        .await
        .expect("saved");
    let login = engine.login_connection(web).await.expect("login");
    assert_eq!(login.username.as_deref(), Some("root"));
    assert_eq!(password(&engine, web).await.as_deref(), Some(SHARED));

    engine
        .set_protected_setting(SHARED_CREDENTIALS_KEY, &false.into())
        .await
        .expect("switched off");
    assert_eq!(
        password(&engine, web).await.as_deref(),
        Some("own-password-of-web")
    );
}

#[tokio::test]
async fn a_new_master_password_re_encrypts_the_entries_with_the_rest() {
    let (_dir, engine) = unlocked().await;
    let shared = save(&engine, create("ops", SHARED)).await;
    let web = connection_on(&engine, "web", shared.id).await;
    engine
        .dispatch(Command::ChangeMasterPassword {
            current: SecretParam::from(MASTER),
            new: SecretParam::from("a new master password"),
        })
        .await
        .expect("changed");
    engine.dispatch(Command::LockVault).await.expect("locked");
    engine
        .dispatch(Command::UnlockVault {
            password: SecretParam::from("a new master password"),
        })
        .await
        .expect("unlocked with the new one");
    assert_eq!(password(&engine, web).await.as_deref(), Some(SHARED));
}

#[tokio::test]
async fn deleting_an_entry_drops_its_secret_and_its_users_fall_back() {
    let (_dir, engine) = unlocked().await;
    let shared = save(&engine, create("ops", SHARED)).await;
    let web = connection_on(&engine, "web", shared.id).await;
    engine
        .dispatch(Command::DeleteCredential { id: shared.id })
        .await
        .expect("deleted");
    assert_eq!(password(&engine, web).await, None);
    assert_eq!(
        engine.db().expect("db").credentials().await.expect("list"),
        []
    );
}

#[tokio::test]
async fn an_agent_neither_fills_an_entry_nor_points_a_connection_at_one() {
    let (_dir, engine) = unlocked().await;
    let shared = save(&engine, create("ops", SHARED)).await;
    let refused = engine
        .dispatch_mcp(Command::SaveCredential(create("agent", "agent-password")))
        .await
        .expect_err("a secret");
    assert_eq!(refused.code(), "gui_only");
    let user = SaveCredentialRequest {
        fields: CredentialFields::Update {
            id: shared.id,
            patch: CredentialPatch {
                username: Some("root".into()),
                ..CredentialPatch::default()
            },
        },
        secrets: Vec::new(),
    };
    let refused = engine
        .dispatch_mcp(Command::SaveCredential(user))
        .await
        .expect_err("the user");
    assert_eq!(refused.code(), "gui_only");
    let link = SaveConnectionRequest {
        fields: SaveFields::Create {
            folder_id: None,
            connection: ConnectionDraft::new("evil", "attacker.example"),
        },
        secrets: Vec::new(),
        login_script: None,
        favourite: None,
        credential: Some(Some(shared.id)),
    };
    let refused = engine
        .dispatch_mcp(Command::SaveConnection(link))
        .await
        .expect_err("a link");
    assert_eq!(refused.code(), "gui_only");

    // A connection on an entry keeps its endpoint from an agent.
    let web = connection_on(&engine, "web", shared.id).await;
    let moved = Command::UpdateConnection {
        id: web,
        patch: ConnectionPatch {
            host: Some("attacker.example".into()),
            ..ConnectionPatch::default()
        },
    };
    let refused = engine.dispatch_mcp(moved).await.expect_err("the endpoint");
    assert_eq!(refused.code(), "endpoint_change_needs_gui");

    // Renaming and adding without a secret are an agent's to do.
    let named = SaveCredentialRequest {
        secrets: Vec::new(),
        ..create("from an agent", "unused")
    };
    let Ok(Outcome::Credential { credential }) =
        engine.dispatch_mcp(Command::SaveCredential(named)).await
    else {
        panic!("added over MCP");
    };
    assert!(!credential.secrets.has_password);
}

#[tokio::test]
async fn the_entry_and_its_users_are_announced() {
    let (_dir, engine) = unlocked().await;
    let mut events = engine.subscribe();
    let shared = save(&engine, create("ops", SHARED)).await;
    let item = TreeItem::Credential(shared.id);
    assert!(
        std::iter::from_fn(|| events.try_recv().ok()).any(|event| event
            == crate::EngineEvent::ConnectionTree {
                item,
                change: TreeChange::Created,
            })
    );
    let tree = engine
        .db()
        .expect("db")
        .connection_tree()
        .await
        .expect("tree");
    assert_eq!(tree.credentials.len(), 1);
    // The value never shows in the entry as the tree, MCP or a log line writes it.
    let shown = format!("{tree:?} {}", serde_json::to_string(&tree).expect("json"));
    assert!(!shown.contains(SHARED), "{shown}");
}
