//! `ssh_exec` end to end, over an in-process pipe and against the in-process SSH server, with
//! the connection's canary password in the vault: the confirmation code, the policy, the
//! question in the window, and the vault's password masked in the output.

use std::time::Duration;

use rc_db::Db;
use rc_engine::{
    Command, CommandClass, ConnectionDraft, EngineEvent, McpAnswer, Outcome, SecretKind,
    SecretParam,
};
use rc_remote::{
    OfferedKey,
    testkit::{PASSWORD, ServerOptions, TestServer, host_key_a},
    trust,
};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};

use crate::test_support::{Built, Fixture, connect};
use crate::{McpContext, Policy, Rule};

const COMMAND: &str = "sh -c 'echo out; echo err >&2; exit 3'";

struct Setup {
    _root: tempfile::TempDir,
    server: TestServer,
    db: Db,
    context: McpContext,
    id: i64,
}

/// An engine with an unlocked vault holding the canary password of a trusted connection to the
/// test server; headless, with confirmation codes allowed.
async fn setup() -> Setup {
    let Built {
        root,
        engine,
        db,
        context,
    } = Fixture::new()
        .with_agent_sources(Vec::new())
        .with_created_vault()
        .with_codes()
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
    let offered = OfferedKey::from_public_key(host_key_a().public_key()).expect("a key");
    trust::trust(&db, "127.0.0.1", server.port, offered)
        .await
        .expect("trusted");
    Setup {
        _root: root,
        server,
        db,
        context,
        id: connection.id.0,
    }
}

/// The answer's JSON; its text never carries the canary password.
async fn call(client: &RunningService<RoleClient, ()>, arguments: Value) -> Value {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = client
        .call_tool(CallToolRequestParams::new("ssh_exec").with_arguments(arguments))
        .await
        .expect("call");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    assert!(!text.contains(PASSWORD), "the password left: {text}");
    serde_json::from_str(&text).expect("json")
}

fn code_of(asked: &Value) -> String {
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    asked["confirmation"].as_str().expect("code").to_owned()
}

fn exec(id: i64, command: &str, code: Option<&str>) -> Value {
    let mut arguments = json!({ "connection_id": id, "command": command });
    if let Some(code) = code {
        arguments["confirmation"] = json!(code);
    }
    arguments
}

#[tokio::test]
async fn a_code_counts_once_for_one_command_on_one_connection() {
    let setup = setup().await;
    let client = connect(&setup.context).await;

    let asked = call(&client, exec(setup.id, COMMAND, None)).await;
    assert_eq!(asked["class"], "exec", "{asked}");
    let code = code_of(&asked);
    let other_command = call(&client, exec(setup.id, "sh -c 'exit 0'", Some(&code))).await;
    code_of(&other_command);
    let code = code_of(&call(&client, exec(setup.id, COMMAND, None)).await);
    code_of(&call(&client, exec(setup.id + 1, COMMAND, Some(&code))).await);
    assert_eq!(
        setup.server.accepts(),
        0,
        "nothing ran without a valid code"
    );

    let code = code_of(&call(&client, exec(setup.id, COMMAND, None)).await);
    let ran = call(&client, exec(setup.id, COMMAND, Some(&code))).await;
    assert_eq!(ran["stdout"], "out\n", "{ran}");
    assert_eq!(ran["stderr"], "err\n", "{ran}");
    assert_eq!(ran["exit_code"], 3, "{ran}");
    assert_eq!(ran["timed_out"], false, "{ran}");
    let again = call(&client, exec(setup.id, COMMAND, Some(&code))).await;
    code_of(&again);
}

#[tokio::test]
async fn deny_for_exec_refuses_every_call_with_its_code() {
    let setup = setup().await;
    Policy {
        exec: Rule::Deny,
        ..Policy::default()
    }
    .store(&setup.db)
    .await
    .expect("stored");
    let client = connect(&setup.context).await;
    let refused = call(&client, exec(setup.id, COMMAND, None)).await;
    assert_eq!(refused["code"], "policy_denied", "{refused}");
    assert_eq!(setup.server.accepts(), 0);
}

#[tokio::test]
async fn ask_shows_the_connection_and_the_command_in_the_window() {
    let mut setup = setup().await;
    setup.context.answer_within(Duration::from_secs(30));
    let engine = setup.context.engine().clone();
    engine.attach_window();
    let mut events = engine.subscribe();
    let person = tokio::spawn(async move {
        loop {
            if let Ok(EngineEvent::McpQuestion { question }) = events.recv().await {
                engine
                    .dispatch(Command::AnswerMcpQuestion {
                        question: question.id,
                        answer: McpAnswer::Allow,
                    })
                    .await
                    .expect("the window answers");
                return *question;
            }
        }
    });
    let client = connect(&setup.context).await;
    let ran = call(&client, exec(setup.id, COMMAND, None)).await;
    assert_eq!(ran["exit_code"], 3, "{ran}");
    let question = person.await.expect("joins");
    assert_eq!(question.tool, "ssh_exec");
    assert_eq!(question.class, CommandClass::Exec);
    let shown = |name: &str| {
        question
            .arguments
            .iter()
            .find(|argument| argument.name == name)
            .map(|argument| argument.value.clone())
    };
    assert_eq!(shown("connection_id"), Some(setup.id.to_string()));
    assert_eq!(shown("command").as_deref(), Some(COMMAND));
}

#[tokio::test]
async fn a_command_carrying_the_vault_password_is_refused_before_the_question() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    // The question would have to show it masked, and nobody can allow what they cannot read
    // (RC-080-34): refused, no code handed out, nothing run.
    let command = format!("sh -c 'echo {PASSWORD}'");
    let refused = call(&client, exec(setup.id, &command, None)).await;
    assert_eq!(refused["code"], "invalid_params", "{refused}");
    assert!(refused.get("confirmation").is_none(), "{refused}");
    assert_eq!(setup.server.accepts(), 0);
}

#[tokio::test]
async fn a_command_the_question_cannot_show_in_full_is_refused() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let long = format!("echo {}", "x".repeat(1000));
    // Refused before the question (RC-080-12): no code is handed out for it.
    let refused = call(&client, exec(setup.id, &long, None)).await;
    assert_eq!(refused["code"], "invalid_params", "{refused}");
    assert!(refused.get("confirmation").is_none(), "{refused}");
    assert_eq!(setup.server.accepts(), 0);
}
