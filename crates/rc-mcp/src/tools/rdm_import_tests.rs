//! `import_remote_desktop_manager` end to end, over an in-process pipe like `tc_import_tests.rs`,
//! on `rc-engine`'s fixture: the default is a dry run that changes nothing, a write policy that
//! denies writes stops the preview too, only `.rdm` files are read, and no answer carries a
//! password.

use std::{fs, path::PathBuf};

use rc_db::Db;
use rc_engine::{Command, Outcome};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::test_support::{Fixture, connect};
use crate::{McpContext, Policy, Rule};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-engine/tests/fixtures/rdm/sample.rdm"
);
/// The fixture's `SafePassword` and the one of its password history.
const CANARY: &str = "cmRtLWNhbmFyeS03ZjNhOQ==";
const HISTORY_CANARY: &str = "cmRtLWhpc3RvcnktYzRlMQ==";

struct Setup {
    _root: TempDir,
    db: Db,
    context: McpContext,
    export: PathBuf,
}

async fn setup() -> Setup {
    let fixture = Fixture::new();
    let export = fixture.root().join("export.rdm");
    fs::copy(FIXTURE, &export).expect("fixture");
    let built = fixture.build().await;
    Setup {
        _root: built.root,
        context: built.context,
        db: built.db,
        export,
    }
}

async fn call(client: &RunningService<RoleClient, ()>, arguments: Value) -> (String, Value) {
    call_tool(client, "import_remote_desktop_manager", arguments).await
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

fn no_password(text: &str) {
    assert!(
        !text.contains(CANARY) && !text.contains(HISTORY_CANARY),
        "{text}"
    );
}

#[tokio::test]
async fn the_default_is_a_preview_without_a_password() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let (text, answer) = call(
        &client,
        json!({ "path": setup.export.display().to_string() }),
    )
    .await;
    assert_eq!(answer["dry_run"], true, "{answer}");
    assert_eq!(answer["entries"][0]["status"], "new", "{answer}");
    assert_eq!(
        answer["entries"][0]["password_state"], "obfuscated",
        "{answer}"
    );
    assert_eq!(
        answer["entries"][3]["code"], "rdm_not_supported_yet",
        "{answer}"
    );
    no_password(&text);
    assert_eq!(connection_count(&setup.context).await, 0);
}

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
    let path = setup.export.display().to_string();
    let (_, preview) = call(&client, json!({ "path": path })).await;
    assert_eq!(preview["code"], "policy_denied", "{preview}");
    let (_, denied) = call(&client, json!({ "path": path, "dry_run": false })).await;
    assert_eq!(denied["code"], "policy_denied", "{denied}");
    assert_eq!(connection_count(&setup.context).await, 0);

    Policy::default().store(&setup.db).await.expect("stores");
    let (text, done) = call(&client, json!({ "path": path, "dry_run": false })).await;
    assert_eq!(done["entries"][0]["status"], "created", "{done}");
    no_password(&text);
    assert_eq!(connection_count(&setup.context).await, 3);
}

#[tokio::test]
async fn only_files_ending_in_rdm_are_read() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let renamed = setup.export.with_file_name("export.xml");
    fs::copy(&setup.export, &renamed).expect("copy");
    for path in [renamed.display().to_string(), "/etc/passwd".to_owned()] {
        let (_, refused) = call(&client, json!({ "path": path })).await;
        assert_eq!(refused["code"], "rdm_path_refused", "{refused}");
    }
}

/// An RDM export keeps its passwords merely obfuscated - no reading tool answers it, while the
/// import, which never answers a password, still reads it (as `wcx_ftp.ini`, RC-080-12).
#[tokio::test]
async fn the_reading_tools_refuse_an_rdm_export_and_the_import_still_reads_it() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let path = setup.export.display().to_string();
    for tool in ["files_read_text", "files_read_hex"] {
        let (text, refused) = call_tool(&client, tool, json!({ "path": path })).await;
        assert_eq!(refused["code"], "sensitive_path", "{tool}: {refused}");
        no_password(&text);
    }
    let (_, preview) = call(&client, json!({ "path": path })).await;
    assert_eq!(preview["entries"][0]["status"], "new", "{preview}");
}
