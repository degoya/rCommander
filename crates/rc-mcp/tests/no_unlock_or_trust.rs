//! No MCP way unlocks the vault or trusts a host key or certificate (RC-080-03), held over the
//! whole tool list rather than one tool at a time.
//!
//! The decisions are commands of the class `gui_only` (`PINNED_GUI_ONLY`); no tool covers
//! one, and `Engine::dispatch_mcp` refuses them besides. The sweep calls every tool the server
//! offers - without arguments, and with arguments that offer the master password and ask for
//! trust - against a locked vault and an SSH server whose host key nobody trusted. Afterwards
//! the vault is still locked, no host key or certificate is trusted, and the server never saw a
//! login attempt: no secret went to a host only a person can vouch for.

use std::time::Duration;

use rc_engine::{COMMANDS, Command, CommandClass, ConnectionId, SecretKind, SecretParam};
use rc_mcp::{
    McpContext, RcMcpServer,
    coverage::{COVERAGE, Coverage},
};
use rc_remote::testkit::{ServerOptions, TestServer, host_key_a};
use rmcp::{RoleClient, ServiceExt as _, model::CallToolRequestParams, service::RunningService};
use serde_json::{Map, Value, json};

#[path = "../src/test_support.rs"]
mod test_support;

use test_support::{Built, Fixture};

const MASTER: &str = "sweep-master-91c4";
const PASSWORD: &str = "sweep-password-0b6e";
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// The decisions only a person makes in the window: the vault, secrets, trust, the answers to
/// the window's questions, restoring the database, exporting and importing settings and secrets.
const PINNED_GUI_ONLY: &[&str] = &[
    "archive.password",
    "certificates.answer",
    "certificates.remove",
    "config.export",
    "config.import",
    "connections.quick_connect",
    "db.backup_to",
    "db.restore",
    "hostkeys.answer",
    "hostkeys.import_known_hosts",
    "hostkeys.remove",
    "hostkeys.replace",
    "import.total_commander_passwords",
    "login_scripts.clear_secret",
    "login_scripts.set_secret",
    "mcp.answer",
    "mcp.revoke",
    "secrets.import_key",
    "vault.change_password",
    "vault.create",
    "vault.remove",
    "vault.unlock",
];

#[test]
fn the_vault_secret_and_trust_decisions_stay_gui_only_and_no_tool_covers_them() {
    for id in PINNED_GUI_ONLY {
        let class = COMMANDS
            .iter()
            .find(|spec| spec.id == *id)
            .map(|spec| spec.class);
        assert_eq!(class, Some(CommandClass::GuiOnly), "{id}");
        let row = COVERAGE.iter().find(|(row, _)| row == id).map(|(_, c)| *c);
        assert!(
            matches!(row, Some(Coverage::Omitted(_))),
            "{id} has a tool: {row:?}"
        );
    }
}

async fn connect(context: &McpContext) -> RunningService<RoleClient, ()> {
    let (server_side, client_side) = tokio::io::duplex(256 * 1024);
    let server = RcMcpServer::new(context.clone());
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_side).await.expect("client")
}

/// One call's answer text; a protocol error or a timeout answers its own text.
async fn call(client: &RunningService<RoleClient, ()>, tool: &str, arguments: &Value) -> String {
    let request = CallToolRequestParams::new(tool.to_owned())
        .with_arguments(arguments.as_object().cloned().unwrap_or_default());
    match tokio::time::timeout(CALL_TIMEOUT, client.call_tool(request)).await {
        Ok(Ok(answer)) => answer
            .content
            .first()
            .and_then(|content| content.as_text())
            .map(|text| text.text.clone())
            .unwrap_or_default(),
        Ok(Err(error)) => error.to_string(),
        Err(_) => format!("{tool} timed out"),
    }
}

async fn json_of(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> Value {
    serde_json::from_str(&call(client, tool, &arguments).await).expect("json")
}

/// Arguments that offer the master password and ask for trust, aimed at the connection.
fn tempting(names: &[String], web: i64) -> Value {
    let mut arguments = Map::new();
    for (name, value) in [
        ("password", json!(MASTER)),
        ("master_password", json!(MASTER)),
        ("passphrase", json!(MASTER)),
        ("answer", json!("trust_and_save")),
        ("trust", json!(true)),
        ("accept", json!(true)),
        ("question", json!(1)),
    ] {
        arguments.insert(name.to_owned(), value);
    }
    for (name, value) in [
        ("id", json!(web)),
        ("connection_id", json!(web)),
        ("path", json!("sftp://web/")),
    ] {
        if names.iter().any(|known| known == name) {
            arguments.insert(name.to_owned(), value);
        }
    }
    Value::Object(arguments)
}

/// Every tool, without arguments and with the tempting ones.
async fn sweep(client: &RunningService<RoleClient, ()>, web: i64) -> usize {
    let tools = client.list_all_tools().await.expect("tools");
    for tool in &tools {
        let names: Vec<String> = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|properties| properties.keys().cloned().collect())
            .unwrap_or_default();
        call(client, &tool.name, &json!({})).await;
        call(client, &tool.name, &tempting(&names, web)).await;
    }
    tools.len()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_tool_call_unlocks_the_vault_or_trusts_a_key_or_certificate() {
    let Built {
        root: _work,
        engine,
        context,
        ..
    } = Fixture::new()
        .with_agent_sources(Vec::new())
        .with_vault()
        .build()
        .await;
    // A server whose host key nobody trusted, with the password it would take.
    let mut server_options = ServerOptions::new(host_key_a());
    server_options.password = Some(PASSWORD.to_owned());
    let server = TestServer::start(server_options).await;

    let client = connect(&context).await;
    let created = json_of(
        &client,
        "connections_create",
        json!({ "name": "web", "host": "127.0.0.1", "port": server.port, "username": "tester", "auth_method": "password" }),
    )
    .await;
    let web = created["id"].as_i64().expect("an id");
    for command in [
        Command::CreateVault {
            password: SecretParam::from(MASTER),
        },
        Command::SaveConnection(rc_engine::SaveConnectionRequest {
            fields: rc_engine::SaveFields::Update {
                id: ConnectionId(web),
                patch: rc_engine::ConnectionPatch::default(),
            },
            secrets: vec![rc_engine::SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(PASSWORD),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }),
    ] {
        engine.dispatch(command).await.expect("the person's step");
    }

    // Unlocked (until the sweep reaches vault_lock): the stored password is at hand, yet
    // nothing reaches the unknown host.
    let refused = json_of(&client, "connections_connect", json!({ "id": web })).await;
    assert_eq!(refused["code"], "host_key_unknown", "{refused}");
    assert!(sweep(&client, web).await > 100, "the whole tool list");
    assert_eq!(server.auth_attempts(), 0, "a login reached an unknown host");

    // Locked: the master password offered to every tool opens nothing.
    let locked = json_of(&client, "vault_lock", json!({})).await;
    assert_eq!(locked["state"], "locked", "{locked}");
    sweep(&client, web).await;

    let status = json_of(&client, "vault_status", json!({})).await;
    assert_eq!(status["state"], "locked", "{status}");
    let keys = json_of(&client, "hostkeys_list", json!({})).await;
    assert_eq!(keys["keys"], json!([]), "{keys}");
    let certificates = json_of(&client, "certificates_list", json!({})).await;
    assert_eq!(certificates["certificates"], json!([]), "{certificates}");
    assert_eq!(server.auth_attempts(), 0, "a login reached an unknown host");
    let still = engine
        .resolve_secret(rc_engine::SecretRef {
            connection: ConnectionId(web),
            kind: SecretKind::Password,
        })
        .await
        .expect_err("locked");
    assert_eq!(still.code(), "vault_locked");
}
