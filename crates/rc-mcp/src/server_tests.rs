//! The gate and the mask end to end: an rmcp client over an in-process pipe, with two test
//! tools no release build has - a `destructive` one and one that plants a secret.

use std::fs;

use rc_db::Db;
use rc_engine::CommandClass;
use rmcp::{
    RoleClient, ServiceExt as _, handler::server::wrapper::Parameters, schemars,
    service::RunningService, tool, tool_router,
};
use serde::Deserialize;
use serde_json::json;
use tempfile::TempDir;

use super::{INSTRUCTIONS, McpContext, RcMcpServer};
use crate::test_support::{Built, Fixture, call};
use crate::{
    Policy, Rule,
    answer::{ToolResult, json_result},
};

const PLANTED: &str = "planted-secret-4711";

#[derive(Deserialize, schemars::JsonSchema)]
struct DestroyParams {
    target: String,
    /// The code a first call answered with.
    #[serde(default)]
    #[allow(dead_code, reason = "read by the gate, not by the tool")]
    confirmation: Option<String>,
}

#[tool_router(router = test_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(description = "Test only: a destructive tool.")]
    async fn test_destroy(&self, Parameters(params): Parameters<DestroyParams>) -> ToolResult {
        json_result(&json!({ "destroyed": params.target }))
    }

    #[tool(description = "Test only: answers with planted credentials.")]
    async fn test_leak(&self) -> ToolResult {
        json_result(&json!({
            "remote": format!("sftp://bob:{PLANTED}@host.example/home"),
            "password": PLANTED,
            "log": format!("Authorization: Bearer {PLANTED}"),
        }))
    }
}

struct Setup {
    _root: TempDir,
    db: Db,
    context: McpContext,
}

async fn setup() -> Setup {
    let fixture = Fixture::new();
    fs::create_dir(fixture.root().join("docs")).expect("mkdir");
    fs::write(fixture.root().join("docs").join("a.txt"), b"a").expect("write");
    // Headless: these tests drive the confirmation-code path (RC-010-13).
    let Built {
        root,
        db,
        mut context,
        ..
    } = fixture.with_codes().build().await;
    context.classify("test_destroy", CommandClass::Destructive);
    context.classify("test_leak", CommandClass::Read);
    Setup {
        _root: root,
        db,
        context,
    }
}

/// A client session over a pipe to a server with the release tools and the test tools.
async fn connect(context: &McpContext) -> RunningService<RoleClient, ()> {
    let (server_side, client_side) = tokio::io::duplex(64 * 1024);
    let server = RcMcpServer::with_router(
        context.clone(),
        RcMcpServer::router() + RcMcpServer::test_router(),
    );
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_side).await.expect("client")
}

#[tokio::test]
async fn a_destructive_tool_wants_a_code_that_works_once() {
    let setup = setup().await;
    let client = connect(&setup.context).await;

    let asked = call(&client, "test_destroy", json!({ "target": "x" })).await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    assert_eq!(asked["class"], "destructive");
    let code = asked["confirmation"].as_str().expect("code").to_owned();

    let done = call(
        &client,
        "test_destroy",
        json!({ "target": "x", "confirmation": code }),
    )
    .await;
    assert_eq!(done["destroyed"], "x", "{done}");

    let again = call(
        &client,
        "test_destroy",
        json!({ "target": "x", "confirmation": code }),
    )
    .await;
    assert_eq!(
        again["confirmation_required"], true,
        "a code is spent: {again}"
    );
    assert_ne!(again["confirmation"], code);
}

#[tokio::test]
async fn a_code_works_for_its_tool_and_session_only() {
    let setup = setup().await;
    let first = connect(&setup.context).await;
    let second = connect(&setup.context).await;

    let asked = call(&first, "test_destroy", json!({ "target": "x" })).await;
    let code = asked["confirmation"].as_str().expect("code").to_owned();
    let elsewhere = call(
        &second,
        "test_destroy",
        json!({ "target": "x", "confirmation": code }),
    )
    .await;
    assert_eq!(elsewhere["confirmation_required"], true, "{elsewhere}");
    let spent = call(
        &first,
        "test_destroy",
        json!({ "target": "x", "confirmation": code }),
    )
    .await;
    assert_eq!(
        spent["confirmation_required"], true,
        "spent by the foreign try"
    );

    let asked = call(&first, "test_destroy", json!({ "target": "x" })).await;
    let code = asked["confirmation"].as_str().expect("code").to_owned();
    // A read tool ignores the field, and the code stays unspent for its own tool.
    let state = call(&first, "get_panel_state", json!({ "confirmation": code })).await;
    assert_eq!(state["panel"], "left");
    let done = call(
        &first,
        "test_destroy",
        json!({ "target": "x", "confirmation": code }),
    )
    .await;
    assert_eq!(done["destroyed"], "x");
}

#[tokio::test]
async fn the_policy_denies_a_class_and_asks_for_another() {
    let setup = setup().await;
    Policy {
        read: Rule::Deny,
        destructive: Rule::Deny,
        ..Policy::default()
    }
    .store(&setup.db)
    .await
    .expect("stores");
    let client = connect(&setup.context).await;
    let denied = call(&client, "list_dir", json!({})).await;
    assert_eq!(denied["code"], "policy_denied", "{denied}");
    let denied = call(&client, "test_destroy", json!({ "target": "x" })).await;
    assert_eq!(denied["code"], "policy_denied", "{denied}");

    Policy {
        read: Rule::Ask,
        ..Policy::default()
    }
    .store(&setup.db)
    .await
    .expect("stores");
    let asked = call(&client, "get_panel_state", json!({})).await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
}

#[tokio::test]
async fn a_planted_secret_never_leaves_the_server() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let answer = call(&client, "test_leak", json!({})).await;
    let rendered = answer.to_string();
    assert!(!rendered.contains(PLANTED), "{rendered}");
    assert_eq!(answer["password"], "[redacted]");
    assert!(rendered.contains("host.example"), "{rendered}");
}

#[tokio::test]
async fn the_panel_tools_drive_the_shared_engine() {
    let setup = setup().await;
    let client = connect(&setup.context).await;

    let tools = client.list_all_tools().await.expect("tools");
    let mut names: Vec<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
    names.sort();
    assert!(names.contains(&"navigate".to_owned()), "{names:?}");

    let listing = call(&client, "list_dir", json!({ "path": "docs" })).await;
    assert_eq!(listing["entries"][0]["name"], "a.txt", "{listing}");
    assert_eq!(listing["entries"][0]["kind"], "file");

    let moved = call(
        &client,
        "navigate",
        json!({ "path": "docs", "panel": "right" }),
    )
    .await;
    assert!(
        moved["path"]["path"]
            .as_str()
            .expect("path")
            .ends_with("docs"),
        "{moved}"
    );
    let right = setup.context.engine().panel(rc_engine::PanelId::Right);
    assert!(right.path.path.ends_with("docs"));

    let cursor = call(
        &client,
        "set_cursor",
        json!({ "panel": "right", "name": "a.txt" }),
    )
    .await;
    assert_eq!(cursor["cursor"]["name"], "a.txt", "{cursor}");
    let both = call(
        &client,
        "set_cursor",
        json!({ "index": 0, "name": "a.txt" }),
    )
    .await;
    assert_eq!(both["code"], "invalid_params", "{both}");

    let selected = call(
        &client,
        "toggle_selection",
        json!({ "panel": "right", "names": ["a.txt"] }),
    )
    .await;
    assert_eq!(selected["selection"], json!(["a.txt"]));

    let switched = call(&client, "switch_active_panel", json!({})).await;
    assert_eq!(switched["panel"], "right");
    let state = call(&client, "get_panel_state", json!({})).await;
    assert_eq!(state["active"], true);
    assert_eq!(state["panel"], "right");

    let missing = call(&client, "navigate", json!({ "path": "nope" })).await;
    assert_eq!(missing["code"], "not_found", "{missing}");
}

#[cfg(not(windows))]
#[tokio::test]
async fn the_wsl_tools_answer_an_empty_list_outside_windows() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    for tool in ["wsl_list_distros", "wsl_refresh"] {
        let answer = call(&client, tool, json!({})).await;
        assert_eq!(answer["kind"], "wsl_distros", "{tool}: {answer}");
        assert_eq!(answer["distros"], json!([]), "{tool}: {answer}");
    }
}

/// `instructions.md` names each tool in backticks and only tools the server has, so a renamed or
/// removed one fails here rather than misleading an agent (RC-080-54). A name with an underscore
/// outside backticks is a tool the check would miss, and is refused too.
#[test]
fn the_instructions_name_only_tools_that_exist() {
    let tools = RcMcpServer::tool_names();
    let is_tool = |word: &str| tools.iter().any(|tool| tool == word);
    assert_eq!(
        INSTRUCTIONS.matches('`').count() % 2,
        0,
        "instructions.md leaves a backtick open"
    );
    let mut quoted = 0;
    for (at, part) in INSTRUCTIONS.split('`').enumerate() {
        if at % 2 == 1 {
            assert!(
                is_tool(part),
                "instructions.md names `{part}`, which is no tool"
            );
            quoted += 1;
            continue;
        }
        for word in part.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            assert!(
                !(word.contains('_') && is_tool(word)),
                "instructions.md names the tool {word} without backticks"
            );
        }
    }
    assert!(quoted > 50, "{quoted} tools named");
}
