//! `import_total_commander` end to end, over an in-process pipe like `ssh_import_tests.rs`, on
//! `rc-engine`'s fixtures: the default is a dry run that changes nothing, a write policy that
//! denies writes stops the preview too, and no answer carries a password.

use std::{fs, path::PathBuf};

use rc_db::Db;
use rc_engine::{Command, Outcome};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::test_support::{Fixture, connect};
use crate::{McpContext, Policy, Rule};

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-engine/tests/fixtures/total_commander"
);
/// The fixture's password of "Plain FTP" and its obfuscated form there.
const CANARY: &str = "tc-canary-7f3a9e";
const CANARY_HEX: &str = "38A6F9D04C9E5111B21734ED77EA2FAA12345678";

struct Setup {
    _root: TempDir,
    db: Db,
    context: McpContext,
    wincmd: PathBuf,
}

async fn setup() -> Setup {
    let fixture = Fixture::new();
    let wincmd = fixture.root().join("wincmd.ini");
    fs::copy(PathBuf::from(FIXTURES).join("wincmd_ansi.ini"), &wincmd).expect("wincmd");
    fs::copy(
        PathBuf::from(FIXTURES).join("wcx_ftp.ini"),
        fixture.root().join("wcx_ftp.ini"),
    )
    .expect("wcx_ftp");
    let built = fixture.build().await;
    Setup {
        _root: built.root,
        context: built.context,
        db: built.db,
        wincmd,
    }
}

async fn call(client: &RunningService<RoleClient, ()>, arguments: Value) -> (String, Value) {
    call_tool(client, "import_total_commander", arguments).await
}

async fn call_tool(
    client: &RunningService<RoleClient, ()>,
    tool: &str,
    arguments: Value,
) -> (String, Value) {
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
    let value = serde_json::from_str(&text).expect("json");
    (text, value)
}

async fn connection_count(context: &McpContext) -> usize {
    match context
        .engine()
        .dispatch(Command::ListConnections)
        .await
        .expect("list")
    {
        Outcome::ConnectionTree { tree } => {
            fn count(folders: &[rc_engine::FolderNode]) -> usize {
                folders
                    .iter()
                    .map(|folder| folder.connections.len() + count(&folder.folders))
                    .sum()
            }
            tree.connections.len() + count(&tree.folders)
        }
        other => panic!("the tree, got {other:?}"),
    }
}

#[tokio::test]
async fn the_default_is_a_preview_without_a_password() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let (text, answer) = call(
        &client,
        json!({ "wincmd_ini": setup.wincmd.display().to_string(), "tabs": false }),
    )
    .await;
    assert_eq!(answer["dry_run"], true, "{answer}");
    assert_eq!(
        answer["connections"][0]["password_state"], "present",
        "{answer}"
    );
    assert_eq!(answer["connections"][0]["status"], "new", "{answer}");
    assert!(
        !text.contains(CANARY) && !text.contains(CANARY_HEX),
        "{text}"
    );
    assert_eq!(connection_count(&setup.context).await, 0);
}

/// RC-080-12 (X7): the import writes, so it is a write tool, its preview included - as the
/// ssh_config import is.
#[tokio::test]
async fn with_writes_denied_neither_the_preview_nor_the_import_runs() {
    let setup = setup().await;
    Policy {
        write: Rule::Deny,
        ..Policy::default()
    }
    .store(&setup.db)
    .await
    .expect("stores");
    let client = connect(&setup.context).await;
    let wincmd = setup.wincmd.display().to_string();
    let (_, preview) = call(&client, json!({ "wincmd_ini": wincmd, "tabs": false })).await;
    assert_eq!(preview["code"], "policy_denied", "{preview}");
    let (_, denied) = call(
        &client,
        json!({ "wincmd_ini": wincmd, "tabs": false, "dry_run": false }),
    )
    .await;
    assert_eq!(denied["code"], "policy_denied", "{denied}");
    assert_eq!(connection_count(&setup.context).await, 0);

    Policy::default().store(&setup.db).await.expect("stores");
    let (text, done) = call(
        &client,
        json!({ "wincmd_ini": wincmd, "tabs": false, "hotlist": false, "dry_run": false }),
    )
    .await;
    assert_eq!(done["connections"][0]["status"], "created", "{done}");
    assert!(
        !text.contains(CANARY) && !text.contains(CANARY_HEX),
        "{text}"
    );
    assert_eq!(connection_count(&setup.context).await, 6);
}

#[tokio::test]
async fn only_files_of_the_two_names_are_read() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let (_, refused) = call(&client, json!({ "wincmd_ini": "/etc/passwd" })).await;
    assert_eq!(refused["code"], "tc_ini_path_refused", "{refused}");
}

/// RC-080-12 (M7): `wcx_ftp.ini` keeps its passwords merely obfuscated - no reading tool answers
/// it, while the import, which never answers a value, still reads it.
#[tokio::test]
async fn the_reading_tools_refuse_wcx_ftp_ini_and_the_import_still_reads_it() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let ftp_ini = setup.wincmd.with_file_name("wcx_ftp.ini");
    for tool in ["files_read_text", "files_read_hex"] {
        let (text, refused) = call_tool(
            &client,
            tool,
            json!({ "path": ftp_ini.display().to_string() }),
        )
        .await;
        assert_eq!(refused["code"], "sensitive_path", "{tool}: {refused}");
        assert!(!text.contains(CANARY_HEX), "{tool}: {text}");
    }
    let (_, preview) = call(
        &client,
        json!({
            "wincmd_ini": setup.wincmd.display().to_string(),
            "ftp_ini": ftp_ini.display().to_string(),
            "tabs": false,
        }),
    )
    .await;
    assert_eq!(
        preview["connections"][0]["password_state"], "present",
        "{preview}"
    );
}
