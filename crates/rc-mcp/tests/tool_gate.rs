//! The gate over the whole tool list (RC-080-03), not over a test tool: every tool whose command
//! is `destructive` or `exec`, and every call another tool escalates to one of these classes, is
//! refused without a person's answer. Headless that is `headless_ask_denied` by default; where a
//! person allowed confirmation codes, a question whose code answers that one call, in that
//! session, once.
//!
//! The gated tools are read from the coverage rows and the registry's classes, so a tool that
//! joins these classes is tested here without a line of its own. `KNOWN_GATED` pins the ones
//! the security review found: a tool that leaves the classes fails here and needs a reason.

use std::collections::BTreeMap;

use rc_engine::{COMMANDS, CommandClass};
use rc_mcp::{
    McpContext, RcMcpServer,
    coverage::{COVERAGE, Coverage},
};
use rmcp::{RoleClient, ServiceExt as _, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

#[path = "../src/test_support.rs"]
mod test_support;

use test_support::Fixture;

/// The destructive and exec tools of the review (2026-10-18).
const KNOWN_GATED: &[&str] = &[
    "connections_delete",
    "connections_folder_delete",
    "diagnostics_clear_error_log",
    "file_open",
    "hotlist_delete",
    "login_scripts_assign",
    "login_scripts_delete",
    "multi_rename_preset_delete",
    "ops_delete",
    "ssh_exec",
    "terminal_close",
    "terminal_open",
    "terminal_send_input",
];

/// Calls of write tools whose arguments make them destructive or exec
/// (`Engine::effective_class`), with the class the question must name.
fn escalated_calls() -> Vec<(&'static str, Value, &'static str)> {
    vec![
        (
            "ops_copy",
            json!({ "sources": ["a.txt"], "target": "docs", "on_conflict": "overwrite" }),
            "destructive",
        ),
        (
            "ops_move",
            json!({ "sources": ["a.txt"], "target": "docs", "on_conflict": "overwrite_older" }),
            "destructive",
        ),
        (
            "archives_extract",
            json!({ "on_conflict": "overwrite" }),
            "destructive",
        ),
        (
            "archives_pack",
            json!({ "delete_sources": true }),
            "destructive",
        ),
        ("archives_pack", json!({ "overwrite": true }), "destructive"),
        (
            "dirs_sync_apply",
            json!({ "allow_deletes": true }),
            "destructive",
        ),
        (
            "dirs_sync_apply",
            json!({ "overwrite": true }),
            "destructive",
        ),
        (
            "login_scripts_create",
            json!({ "name": "gate", "steps": [{ "step": "send_line" }] }),
            "exec",
        ),
        (
            "login_scripts_update",
            json!({ "id": 1, "steps": [] }),
            "exec",
        ),
        // Below the limit in force (`Engine::effective_class`, RC-080-12).
        (
            "diagnostics_set_retention",
            json!({ "table": "error_log", "max_rows": 100 }),
            "destructive",
        ),
    ]
}

/// Each served tool's class: the most dangerous class of the commands it covers.
fn tool_classes() -> BTreeMap<String, CommandClass> {
    let mut classes = BTreeMap::new();
    for (id, coverage) in COVERAGE {
        let Coverage::Covered(tool) = coverage else {
            continue;
        };
        let class = COMMANDS
            .iter()
            .find(|spec| spec.id == *id)
            .map(|spec| spec.class)
            .expect("every coverage row names a command");
        let entry = classes.entry((*tool).to_owned()).or_insert(class);
        *entry = (*entry).stricter(class);
    }
    classes
}

/// Every tool the gate must ask about whatever its arguments.
fn gated_tools() -> Vec<(String, &'static str)> {
    tool_classes()
        .into_iter()
        .filter(|(_, class)| matches!(class, CommandClass::Destructive | CommandClass::Exec))
        .map(|(tool, class)| (tool, class.as_str()))
        .collect()
}

/// Every gated call: the gated tools without arguments, then the escalated calls.
fn gated_calls() -> Vec<(String, Value, &'static str)> {
    let mut calls: Vec<(String, Value, &'static str)> = gated_tools()
        .into_iter()
        .map(|(tool, class)| (tool, json!({}), class))
        .collect();
    calls.extend(
        escalated_calls()
            .into_iter()
            .map(|(tool, arguments, class)| (tool.to_owned(), arguments, class)),
    );
    calls
}

struct Setup {
    root: TempDir,
    context: McpContext,
}

/// A headless engine over a folder with `a.txt` and `docs/`; confirmation codes allowed or not.
async fn setup(codes: bool) -> Setup {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.root().join("docs")).expect("mkdir");
    std::fs::write(fixture.root().join("a.txt"), b"a").expect("write");
    let fixture = if codes { fixture.with_codes() } else { fixture };
    let built = fixture.build().await;
    Setup {
        root: built.root,
        context: built.context,
    }
}

async fn connect(context: &McpContext) -> RunningService<RoleClient, ()> {
    let (server_side, client_side) = tokio::io::duplex(64 * 1024);
    let server = RcMcpServer::new(context.clone());
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_side).await.expect("client")
}

/// One call's answer as JSON.
async fn call(client: &RunningService<RoleClient, ()>, tool: &str, arguments: &Value) -> Value {
    let result = client
        .call_tool(
            CallToolRequestParams::new(tool.to_owned())
                .with_arguments(arguments.as_object().cloned().unwrap_or_default()),
        )
        .await
        .expect("call");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("a text block");
    serde_json::from_str(&text).expect("json")
}

fn with_code(arguments: &Value, code: &str) -> Value {
    let mut arguments = arguments.clone();
    arguments["confirmation"] = json!(code);
    arguments
}

/// The code of an answer that asks, after checking it asks about `tool` as `class`.
fn code_of(answer: &Value, tool: &str, class: &str) -> String {
    assert_eq!(answer["confirmation_required"], true, "{tool}: {answer}");
    assert_eq!(answer["tool"], tool, "{answer}");
    assert_eq!(answer["class"], class, "{tool}: {answer}");
    let code = answer["confirmation"].as_str().expect("a code").to_owned();
    assert_eq!(code.len(), 32, "{code}");
    code
}

#[test]
fn the_gated_tools_are_served_and_hold_the_reviewed_ones() {
    let gated: Vec<String> = gated_tools().into_iter().map(|(tool, _)| tool).collect();
    for known in KNOWN_GATED {
        assert!(
            gated.iter().any(|tool| tool == known),
            "{known} is no longer destructive or exec; a security review decides that: {gated:?}"
        );
    }
    let served = RcMcpServer::tool_names();
    for (tool, _, _) in gated_calls() {
        assert!(served.contains(&tool), "{tool} is not served");
    }
}

#[tokio::test]
async fn headless_without_codes_every_gated_call_is_refused() {
    let setup = setup(false).await;
    let client = connect(&setup.context).await;
    for (tool, arguments, _) in gated_calls() {
        let answer = call(&client, &tool, &arguments).await;
        assert_eq!(answer["code"], "headless_ask_denied", "{tool}: {answer}");
        // A code the agent made up changes nothing.
        let forged = with_code(&arguments, "0123456789abcdef0123456789abcdef");
        let answer = call(&client, &tool, &forged).await;
        assert_eq!(answer["code"], "headless_ask_denied", "{tool}: {answer}");
    }
}

#[tokio::test]
async fn with_codes_every_gated_call_asks_and_a_forged_code_does_not_answer() {
    let setup = setup(true).await;
    let client = connect(&setup.context).await;
    for (tool, arguments, class) in gated_calls() {
        code_of(&call(&client, &tool, &arguments).await, &tool, class);
        let forged = with_code(&arguments, "0123456789abcdef0123456789abcdef");
        code_of(&call(&client, &tool, &forged).await, &tool, class);
    }
}

#[tokio::test]
async fn a_code_answers_no_other_gated_call() {
    let setup = setup(true).await;
    let client = connect(&setup.context).await;
    let calls = gated_calls();
    assert!(calls.len() >= 2);
    let mut codes = Vec::new();
    for (tool, arguments, class) in &calls {
        codes.push(code_of(&call(&client, tool, arguments).await, tool, class));
    }
    // The code of each call offered to the next one, whose tool or arguments differ: it asks
    // again, and nothing runs.
    for (at, code) in codes.iter().enumerate() {
        let (tool, arguments, class) = &calls[(at + 1) % calls.len()];
        let answer = call(&client, tool, &with_code(arguments, code)).await;
        code_of(&answer, tool, class);
    }
}

#[tokio::test]
async fn a_code_is_bound_to_every_argument_of_its_call() {
    let setup = setup(true).await;
    let client = connect(&setup.context).await;
    for (tool, asked, offered, class) in [
        (
            "ops_delete",
            json!({ "sources": ["a.txt"] }),
            json!({ "sources": ["docs"] }),
            "destructive",
        ),
        (
            "ops_delete",
            json!({ "sources": ["a.txt"] }),
            json!({ "sources": ["a.txt"], "permanent": true }),
            "destructive",
        ),
        (
            "connections_delete",
            json!({ "id": 1 }),
            json!({ "id": 2 }),
            "destructive",
        ),
        (
            "hotlist_delete",
            json!({ "id": 1 }),
            json!({ "id": 2 }),
            "destructive",
        ),
        (
            "terminal_send_input",
            json!({ "terminal": 1, "text": "ls\r" }),
            json!({ "terminal": 1, "text": "rm -rf ~\r" }),
            "exec",
        ),
    ] {
        let code = code_of(&call(&client, tool, &asked).await, tool, class);
        let answer = call(&client, tool, &with_code(&offered, &code)).await;
        code_of(&answer, tool, class);
    }
    assert!(
        setup.root.path().join("a.txt").exists(),
        "nothing was deleted"
    );
}

#[tokio::test]
async fn a_code_is_spent_by_its_use_and_belongs_to_its_session() {
    let setup = setup(true).await;
    let client = connect(&setup.context).await;
    let tool = "diagnostics_clear_error_log";
    let arguments = json!({});

    let code = code_of(&call(&client, tool, &arguments).await, tool, "destructive");
    let done = call(&client, tool, &with_code(&arguments, &code)).await;
    assert_ne!(
        done["confirmation_required"], true,
        "the code answered: {done}"
    );
    assert!(done.get("code").is_none(), "it ran: {done}");
    let again = call(&client, tool, &with_code(&arguments, &code)).await;
    code_of(&again, tool, "destructive");

    let other = connect(&setup.context).await;
    let code = code_of(&call(&client, tool, &arguments).await, tool, "destructive");
    let elsewhere = call(&other, tool, &with_code(&arguments, &code)).await;
    code_of(&elsewhere, tool, "destructive");
}
