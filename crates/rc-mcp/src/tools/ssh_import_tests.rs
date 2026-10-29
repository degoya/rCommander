//! `connections_import_ssh_config` end to end, over an in-process pipe like `server_tests.rs`.
//!
//! The tool reads only the real user's `~/.ssh/config` (and `config.d`), which a test cannot
//! own; the import itself is tested in `rc-engine` with a home of its own. Here: the default is
//! a dry run, no other file is read, and no key is taken into the vault (RC-050-06): that is
//! the window's, after the person ticks it.

use std::{fs, path::PathBuf};

use rc_engine::{AuthMethod, Command, ConnectionDraft, ImportKeyRequest, Outcome, VaultStateKind};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, connect};

/// PEM armour, split so that no secret scanner takes the test for a leaked key.
const ARMOUR: &str = concat!("-----BEGIN ", "OPENSSH PRIVATE KEY-----\n");

const CONFIG: &str = "\
Host web
    HostName web.example.org
    User deploy
    IdentityFile ~/.ssh/id_web
    PasswordAuthentication no

Host *
    User everyone
";

struct Setup {
    _root: TempDir,
    context: McpContext,
    config: PathBuf,
}

async fn setup() -> Setup {
    let fixture = Fixture::new();
    let ssh = fixture.root().join("home/tester/.ssh");
    fs::create_dir_all(&ssh).expect("ssh dir");
    let config = ssh.join("config");
    fs::write(&config, CONFIG).expect("config");
    let built = fixture.build().await;
    Setup {
        _root: built.root,
        context: built.context,
        config,
    }
}

async fn call(client: &RunningService<RoleClient, ()>, arguments: Value) -> Value {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = client
        .call_tool(
            CallToolRequestParams::new("connections_import_ssh_config".to_owned())
                .with_arguments(arguments),
        )
        .await
        .expect("call");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

/// How many connections the engine holds, read past MCP.
async fn connection_count(context: &McpContext) -> usize {
    match context
        .engine()
        .dispatch(Command::ListConnections)
        .await
        .expect("list")
    {
        Outcome::ConnectionTree { tree } => {
            tree.connections.len()
                + tree
                    .folders
                    .iter()
                    .map(|folder| folder.connections.len())
                    .sum::<usize>()
        }
        other => panic!("the tree, got {other:?}"),
    }
}

/// Without arguments the tool previews the user's own `~/.ssh/config`: whatever this machine
/// holds - a preview, no such file, or a refusal - nothing is written.
#[tokio::test]
async fn the_default_is_a_dry_run_that_changes_nothing() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let answer = call(&client, json!({})).await;
    match answer.get("dry_run") {
        Some(dry_run) => assert_eq!(dry_run, true, "{answer}"),
        None => assert!(
            answer["code"].is_string(),
            "a preview or a refusal: {answer}"
        ),
    }
    assert_eq!(connection_count(&setup.context).await, 0);
}

/// No free path: a configuration anywhere but the user's `~/.ssh` is refused, as is a key, a
/// document or a file that does not exist - all with the same code, so no answer tells
/// whether a file exists.
#[tokio::test]
async fn no_file_but_the_users_own_config_is_read() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let ssh = setup.config.parent().expect("ssh dir").to_path_buf();
    fs::write(ssh.join("id_rsa"), ARMOUR).expect("key");
    for path in [
        setup.config.clone(),
        ssh.join("id_rsa"),
        ssh.join("no-such-file"),
        PathBuf::from("/etc/passwd"),
    ] {
        let refused = call(
            &client,
            json!({ "path": path.display().to_string(), "dry_run": false }),
        )
        .await;
        assert_eq!(
            refused["code"], "ssh_config_path_refused",
            "{path:?}: {refused}"
        );
        let text = refused.to_string();
        assert!(!text.contains("BEGIN"), "{text}");
    }
    assert_eq!(connection_count(&setup.context).await, 0);
}

#[tokio::test]
async fn a_secret_among_the_arguments_is_refused() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let refused = call(&client, json!({ "passphrase": "x" })).await;
    assert_eq!(refused["code"], "secret_not_accepted", "{refused}");
    assert_eq!(connection_count(&setup.context).await, 0);
}

/// The tool has no argument that takes a key, no tool takes a key file into the vault, and the
/// command that does is refused to any MCP caller - the vault is never asked for.
#[tokio::test]
async fn no_key_goes_into_the_vault_over_mcp() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let tools = client.list_all_tools().await.expect("tools");
    let tool = tools
        .iter()
        .find(|tool| tool.name == "connections_import_ssh_config")
        .expect("the tool");
    let mut arguments: Vec<String> = tool
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default();
    arguments.sort();
    assert_eq!(arguments, ["dry_run", "folder_id", "hosts", "path"]);
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert!(
        !names
            .iter()
            .any(|name| name.starts_with("secrets") || name.contains("import_key")),
        "{names:?}"
    );

    // A connection as the import leaves it: a key path, no key.
    let key = setup.config.with_file_name("id_web");
    fs::write(&key, ARMOUR).expect("key");
    let engine = setup.context.engine();
    let draft = ConnectionDraft {
        auth_method: AuthMethod::Key,
        key_path: Some(key.display().to_string()),
        ..ConnectionDraft::new("web", "web.example.org")
    };
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
    else {
        panic!("the connection");
    };
    let refused = engine
        .dispatch_mcp(Command::ImportKey(ImportKeyRequest {
            connection_id: connection.id,
            path: key.display().to_string(),
            passphrase: None,
        }))
        .await
        .expect_err("the person's alone");
    assert_eq!(refused.code(), "gui_only");
    let Ok(Outcome::Vault(status)) = engine.dispatch(Command::GetVaultStatus).await else {
        panic!("the vault's status");
    };
    assert_eq!(status.state, VaultStateKind::Absent);
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::GetConnection { id: connection.id })
        .await
    else {
        panic!("the connection");
    };
    assert!(!connection.secrets.has_key && connection.key.is_none());
}
