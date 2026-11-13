//! `db_backup` over an in-process pipe, against a database file in a temporary data directory.

use rc_db::{BACKUP_DIR, DB_FILE, Db};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::RcMcpServer;
use crate::confirm::CODE_ARGUMENT;
use crate::test_support::Fixture;

struct Setup {
    data: TempDir,
    client: RunningService<RoleClient, ()>,
}

async fn setup() -> Setup {
    let fixture = Fixture::new();
    let db = Db::open(fixture.root().join(DB_FILE)).await.expect("db");
    let built = fixture.with_db(db).build().await;
    let client = built.client().await;
    Setup {
        data: built.root,
        client,
    }
}

fn request(arguments: &Value) -> CallToolRequestParams {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    CallToolRequestParams::new("db_backup".to_owned()).with_arguments(arguments)
}

#[tokio::test]
async fn a_backup_lands_in_the_backup_folder_under_a_generated_name() {
    let setup = setup().await;
    let result = setup
        .client
        .call_tool(request(&json!({})))
        .await
        .expect("call");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    let answer: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(answer["kind"], "database_backup", "{answer}");
    let path = std::path::PathBuf::from(answer["path"].as_str().expect("path"));
    assert_eq!(
        path.parent(),
        Some(setup.data.path().join(BACKUP_DIR).as_path())
    );
    let name = path
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    assert!(rc_db::is_database_file_name(&name), "{name}");
    assert!(path.is_file());
    assert!(answer["size"].as_u64().is_some_and(|size| size > 0));
}

/// No path reaches the command: an argument naming one is refused, and nothing is written there.
#[tokio::test]
async fn the_tool_accepts_no_path() {
    let setup = setup().await;
    let elsewhere = setup.data.path().join("elsewhere.db");
    let call = setup
        .client
        .call_tool(request(&json!({ "path": elsewhere.to_string_lossy() })))
        .await;
    match call {
        Err(_) => {}
        Ok(result) => assert_eq!(result.is_error, Some(true), "{result:?}"),
    }
    assert!(!elsewhere.exists());
    assert!(!setup.data.path().join(BACKUP_DIR).exists());

    let tools = setup.client.list_all_tools().await.expect("tools");
    let tool = tools
        .iter()
        .find(|tool| tool.name == "db_backup")
        .expect("db_backup is served");
    let properties = tool
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(properties, ["confirmation"]);
}

/// The code argument is the one every tool takes (`confirm::argument`), and a call that carries
/// one is not refused as an unknown argument (RC-110-23).
#[tokio::test]
async fn the_code_argument_is_the_shared_one_and_is_accepted() {
    let setup = setup().await;
    let tools = setup.client.list_all_tools().await.expect("tools");
    let schema_of = |name: &str| {
        tools
            .iter()
            .find(|tool| tool.name == name)
            .and_then(|tool| tool.input_schema.get("properties"))
            .and_then(|properties| properties.get(CODE_ARGUMENT))
            .cloned()
            .unwrap_or_else(|| panic!("{name} takes the code"))
    };
    assert_eq!(schema_of("db_backup"), schema_of("ops_delete"));

    let result = setup
        .client
        .call_tool(request(&json!({ (CODE_ARGUMENT): "000000" })))
        .await
        .expect("call");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    let answer: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(answer["kind"], "database_backup", "{answer}");
}

/// A restore replaces every setting and secret: no tool offers one, and a path of its own for a
/// backup neither.
#[test]
fn no_tool_restores_or_backs_up_to_a_path() {
    let names = RcMcpServer::tool_names();
    assert!(names.iter().any(|name| name == "db_backup"));
    assert!(
        !names
            .iter()
            .any(|name| name.contains("restore") || name == "db_backup_to"),
        "{names:?}"
    );
}
