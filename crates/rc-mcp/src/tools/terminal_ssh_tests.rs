//! `terminal_open` with a `connection_id` end to end, over an in-process pipe and against the
//! in-process shell server: the code bound to the connection, the login script's run in
//! `terminal_list`, the password masked on the screen, and a locked vault that asks no one.

use std::time::{Duration, Instant};

use rc_engine::{
    Command, ConnectionDraft, EngineEvent, LoginScriptDraft, Outcome, SecretKind, SecretParam,
    login_script::TEMPLATES,
};
use rc_remote::{
    OfferedKey,
    testkit::{PASSWORD, ShellServer, host_key_a},
    trust,
};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};

use crate::McpContext;
use crate::test_support::{Built, Fixture, connect};

struct Setup {
    _root: tempfile::TempDir,
    _server: ShellServer,
    context: McpContext,
    id: i64,
}

/// A headless engine that answers with confirmation codes, an unlocked vault with the canary
/// password of a connection to the shell server, and the person's `sudo -i` script on it.
async fn setup() -> Setup {
    let Built {
        root,
        engine,
        db,
        context,
    } = Fixture::new()
        .with_codes()
        .with_agent_sources(Vec::new())
        .with_created_vault()
        .build()
        .await;
    let server = ShellServer::start(host_key_a()).await;
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
    let sudo = TEMPLATES
        .iter()
        .find(|template| template.id == "sudo_i")
        .expect("the sudo template")
        .script();
    let Ok(Outcome::LoginScript { script }) = engine
        .dispatch(Command::CreateLoginScript {
            script: LoginScriptDraft {
                name: "to root".into(),
                steps: sudo.steps,
                step_timeout_ms: None,
                total_timeout_ms: None,
            },
        })
        .await
    else {
        panic!("a script");
    };
    engine
        .dispatch(Command::AssignLoginScript {
            connection_id: connection.id,
            script_id: Some(script.id),
        })
        .await
        .expect("assigned");
    let offered = OfferedKey::from_public_key(host_key_a().public_key()).expect("a key");
    trust::trust(&db, "127.0.0.1", server.port, offered)
        .await
        .expect("trusted");
    Setup {
        _root: root,
        _server: server,
        context,
        id: connection.id.0,
    }
}

/// The answer's JSON; no answer ever carries the password.
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

/// Calls `tool` once for its code, then with it.
async fn confirmed(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> Value {
    let asked = call(client, tool, arguments.clone()).await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    let mut arguments = arguments;
    arguments["confirmation"] = asked["confirmation"].clone();
    call(client, tool, arguments).await
}

/// `terminal`'s entry in `terminal_list` once `check` holds for it.
async fn listed_when(
    client: &RunningService<RoleClient, ()>,
    terminal: &Value,
    check: impl Fn(&Value) -> bool,
) -> Value {
    let started = Instant::now();
    loop {
        let listed = call(client, "terminal_list", json!({})).await;
        let entry = listed["terminals"]
            .as_array()
            .and_then(|terminals| {
                terminals
                    .iter()
                    .find(|entry| &entry["terminal"] == terminal)
            })
            .cloned()
            .expect("listed");
        if check(&entry) {
            return entry;
        }
        assert!(started.elapsed() < Duration::from_secs(10), "{entry}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The screen once it shows `needle`.
async fn screen_with(
    client: &RunningService<RoleClient, ()>,
    terminal: &Value,
    needle: &str,
) -> String {
    let started = Instant::now();
    loop {
        let screen = call(
            client,
            "terminal_read_screen",
            json!({ "terminal": terminal, "scrollback": 100 }),
        )
        .await;
        let text = screen.to_string();
        if text.contains(needle) {
            return text;
        }
        assert!(started.elapsed() < Duration::from_secs(10), "{text}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn an_agent_opens_a_root_shell_by_the_person_s_script_and_never_sees_the_password() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let open = |id: i64, code: Option<&Value>| {
        let mut arguments = json!({ "connection_id": id, "cwd": "/srv/my site" });
        if let Some(code) = code {
            arguments["confirmation"] = code.clone();
        }
        arguments
    };
    let asked = call(&client, "terminal_open", open(setup.id, None)).await;
    assert_eq!(asked["class"], "exec", "{asked}");
    let other = call(
        &client,
        "terminal_open",
        open(setup.id + 1, Some(&asked["confirmation"])),
    )
    .await;
    assert_eq!(
        other["confirmation_required"], true,
        "the code is bound to the connection: {other}"
    );

    let opened = confirmed(&client, "terminal_open", open(setup.id, None)).await;
    assert_eq!(opened["shell"], "ssh", "{opened}");
    assert_eq!(opened["connection_id"], setup.id, "{opened}");
    let terminal = opened["terminal"].clone();
    let entry = listed_when(&client, &terminal, |entry| {
        entry["login_script"]["state"] == "completed"
    })
    .await;
    assert_eq!(entry["login_script"]["name"], "to root", "{entry}");
    screen_with(&client, &terminal, "root@test:~#").await;

    // Typing the vault password is refused before the question (RC-080-34): the question would
    // have to mask it, and nobody can allow what they cannot read.
    let refused = call(
        &client,
        "terminal_send_input",
        json!({ "terminal": terminal, "text": format!("echo {PASSWORD}\r") }),
    )
    .await;
    assert_eq!(refused["code"], "invalid_params", "{refused}");
    assert!(refused.get("confirmation").is_none(), "{refused}");
}

#[tokio::test]
async fn a_connection_takes_no_shell_or_distro() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let refused = confirmed(
        &client,
        "terminal_open",
        json!({ "connection_id": setup.id, "shell": "bash" }),
    )
    .await;
    assert_eq!(refused["code"], "invalid_params", "{refused}");
}

#[tokio::test]
async fn a_locked_vault_answers_vault_locked_and_opens_no_dialog() {
    let setup = setup().await;
    setup
        .context
        .engine()
        .dispatch(Command::LockVault)
        .await
        .expect("locked");
    let mut events = setup.context.engine().subscribe();
    let client = connect(&setup.context).await;
    let refused = confirmed(
        &client,
        "terminal_open",
        json!({ "connection_id": setup.id }),
    )
    .await;
    assert_eq!(refused["code"], "vault_locked", "{refused}");
    let asked = std::iter::from_fn(|| events.try_recv().ok())
        .any(|event| matches!(event, EngineEvent::SecretNeeded { .. }));
    assert!(!asked, "an agent's call opens no unlock dialog");
}
