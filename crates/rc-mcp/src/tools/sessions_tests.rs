//! The session tools end to end, over an in-process pipe and against the in-process SSH server,
//! with the canary password in the vault.

use rc_engine::{Command, ConnectionDraft, EngineEvent, Outcome, SecretKind, SecretParam};
use rc_remote::{
    OfferedKey,
    testkit::{PASSWORD, ServerOptions, TestServer, host_key_a},
    trust,
};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};

use crate::McpContext;
use crate::test_support::{Built, Fixture, connect};

struct Setup {
    _root: tempfile::TempDir,
    _server: TestServer,
    context: McpContext,
    id: i64,
}

/// An engine with an unlocked vault holding the canary password of a connection to the test
/// server, whose key is trusted when `trusted`.
async fn setup(trusted: bool) -> Setup {
    let Built {
        root,
        engine,
        db,
        context,
    } = Fixture::new()
        .with_agent_sources(Vec::new())
        .with_created_vault()
        .build()
        .await;
    let server = TestServer::start(ServerOptions::new(host_key_a())).await;
    let draft = ConnectionDraft {
        port: Some(i64::from(server.port)),
        username: Some("tester".to_owned()),
        ..ConnectionDraft::new("box", "127.0.0.1")
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
    engine
        .dispatch(Command::SaveConnection(rc_engine::SaveConnectionRequest {
            fields: rc_engine::SaveFields::Update {
                id: connection.id,
                patch: rc_engine::ConnectionPatch::default(),
            },
            secrets: vec![rc_engine::SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(PASSWORD),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }))
        .await
        .expect("password stored");
    if trusted {
        let offered = OfferedKey::from_public_key(host_key_a().public_key()).expect("a key");
        trust::trust(&db, "127.0.0.1", server.port, offered)
            .await
            .expect("trusted");
    }
    Setup {
        _root: root,
        _server: server,
        context,
        id: connection.id.0,
    }
}

/// The answer's text, checked for the canary, and its JSON.
async fn call(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> Value {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
        .await
        .expect("call");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    assert!(
        !text.contains(PASSWORD),
        "{tool} answered the password: {text}"
    );
    serde_json::from_str(&text).expect("json")
}

#[tokio::test]
async fn connect_test_and_disconnect_never_answer_the_password() {
    let setup = setup(true).await;
    let client = connect(&setup.context).await;
    let connected = call(&client, "connections_connect", json!({"id": setup.id})).await;
    assert_eq!(connected["state"], "connected", "{connected}");
    assert_eq!(connected["login"]["method"], "password", "{connected}");
    let listed = call(&client, "connections_list", json!({})).await;
    assert_eq!(listed["sessions"][0]["connection_id"], setup.id, "{listed}");
    assert_eq!(listed["connections"][0]["has_password"], true, "{listed}");
    let tested = call(&client, "connections_test", json!({"id": setup.id})).await;
    assert_eq!(tested["login"]["method"], "password", "{tested}");
    assert!(tested["duration_ms"].is_u64(), "{tested}");
    let closed = call(&client, "connections_disconnect", json!({"id": setup.id})).await;
    assert_eq!(closed["was_open"], true, "{closed}");
}

#[tokio::test]
async fn a_locked_vault_answers_vault_locked_without_asking_the_person() {
    let setup = setup(true).await;
    setup
        .context
        .engine()
        .dispatch(Command::LockVault)
        .await
        .expect("locked");
    let mut events = setup.context.engine().subscribe();
    let client = connect(&setup.context).await;
    let refused = call(&client, "connections_connect", json!({"id": setup.id})).await;
    assert_eq!(refused["code"], "vault_locked", "{refused}");
    let asked = std::iter::from_fn(|| events.try_recv().ok())
        .any(|event| matches!(event, EngineEvent::SecretNeeded { .. }));
    assert!(!asked, "an agent's call opens no unlock dialog");
}

#[tokio::test]
async fn an_untrusted_server_answers_host_key_unknown() {
    let setup = setup(false).await;
    let client = connect(&setup.context).await;
    let refused = call(&client, "connections_connect", json!({"id": setup.id})).await;
    assert_eq!(refused["code"], "host_key_unknown", "{refused}");
}

#[tokio::test]
async fn settings_change_only_what_is_named() {
    let setup = setup(true).await;
    let client = connect(&setup.context).await;
    let changed = call(
        &client,
        "connections_set_session_prefs",
        json!({"id": setup.id, "auth_order": ["password", "agent"], "keepalive_secs": 10}),
    )
    .await;
    assert_eq!(
        changed["auth_order"],
        json!(["password", "agent"]),
        "{changed}"
    );
    assert_eq!(changed["keepalive_secs"], 10);
    assert_eq!(changed["idle_close_secs"], 600, "unchanged");
    let read = call(
        &client,
        "connections_session_prefs",
        json!({"id": setup.id}),
    )
    .await;
    assert_eq!(read, changed);
    let refused = call(
        &client,
        "connections_set_session_prefs",
        json!({"id": setup.id, "keepalive_secs": 0}),
    )
    .await;
    assert_eq!(refused["code"], "session_prefs_invalid", "{refused}");
}
