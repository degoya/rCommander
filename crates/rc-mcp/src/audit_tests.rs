//! The audit log of the gate end to end (RC-080-66): an rmcp client over an in-process pipe,
//! headless with confirmation codes, and the rows read back from the database.

use rc_db::{MCP_AUDIT_ARGUMENTS_MAX_BYTES, McpAuditEntry};
use rc_engine::settings::{MCP_AUDIT_LOG_KEY, MCP_HEADLESS_CODE_KEY};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};

use super::logged_arguments;
use crate::{
    Policy, Rule,
    test_support::{Built, Fixture, call, connect},
};

/// The canary of RC-080-61's sweep: it must never reach the log.
const CANARY: &str = "canary-password-5f1e9c";

/// Every row, oldest first.
async fn rows(built: &Built) -> Vec<McpAuditEntry> {
    let mut rows = built.db.mcp_audit_log(None, 1_000).await.expect("read");
    rows.reverse();
    rows
}

/// A call to a tool the server does not have: the router's error, whatever it says.
async fn call_unknown(client: &RunningService<RoleClient, ()>) {
    let arguments = json!({ "x": 1 }).as_object().cloned().unwrap_or_default();
    let _ = client
        .call_tool(CallToolRequestParams::new("no_such_tool".to_owned()).with_arguments(arguments))
        .await;
}

#[tokio::test]
async fn every_gate_decision_is_one_row() {
    let built = Fixture::new().with_codes().build().await;
    Policy {
        destructive: Rule::Deny,
        ..Policy::default()
    }
    .store(&built.db)
    .await
    .expect("stores");
    let client = connect(&built.context).await;
    let input = json!({ "terminal": 1, "text": "ls" });

    call(&client, "get_panel_state", json!({ "panel": "left" })).await;
    call(
        &client,
        "connections_get",
        json!({ "id": 1, "password": CANARY }),
    )
    .await;
    call_unknown(&client).await;
    let asked = call(&client, "terminal_send_input", input.clone()).await;
    let mut confirmed = input.clone();
    confirmed["confirmation"] = asked["confirmation"].clone();
    call(&client, "terminal_send_input", confirmed).await;
    call(&client, "terminal_close", json!({ "terminal": 1 })).await;
    built
        .db
        .set(MCP_HEADLESS_CODE_KEY, &false)
        .await
        .expect("stores");
    call(&client, "terminal_send_input", input).await;

    let rows = rows(&built).await;
    let seen: Vec<(&str, Option<&str>, &str)> = rows
        .iter()
        .map(|row| {
            (
                row.tool.as_str(),
                row.class.as_deref(),
                row.decision.as_str(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("get_panel_state", Some("read"), "allowed"),
            ("connections_get", None, "refused"),
            ("no_such_tool", None, "unknown"),
            ("terminal_send_input", Some("exec"), "asked"),
            ("terminal_send_input", Some("exec"), "confirmed"),
            ("terminal_close", Some("destructive"), "denied"),
            ("terminal_send_input", Some("exec"), "declined"),
        ]
    );
    let codes: Vec<&str> = rows.iter().map(|row| row.code.as_str()).collect();
    assert_eq!(codes[0], "ok");
    assert_eq!(codes[1], "secret_not_accepted");
    assert!(codes[2].starts_with("rpc_"), "{}", codes[2]);
    assert_eq!(codes[3], "confirmation_required");
    assert_eq!(codes[4], "terminal_not_found");
    assert_eq!(codes[5], "policy_denied");
    assert_eq!(codes[6], "headless_ask_denied");
    assert!(rows.iter().all(|row| row.session == rows[0].session));
    assert!(rows[0].session.starts_with("instance:"));
    // The redeemed code is not kept; the arguments are.
    assert_eq!(rows[4].arguments, r#"{"terminal":1,"text":"ls"}"#);
}

/// Arguments are masked as an answer is: by a secret's name and in an address. The answer is
/// never written.
#[tokio::test]
async fn arguments_are_masked_and_answers_never_written() {
    let built = Fixture::new().build().await;
    let client = connect(&built.context).await;
    call(
        &client,
        "connections_get",
        json!({ "id": 1, "password": CANARY }),
    )
    .await;
    let address = format!("sftp://user:{CANARY}@example.invalid/home");
    call(&client, "list_dir", json!({ "path": address })).await;
    call(&client, "get_panel_state", json!({})).await;

    let rows = rows(&built).await;
    assert_eq!(rows.len(), 3);
    for row in &rows {
        let whole = serde_json::to_string(row).expect("serializes");
        assert!(!whole.contains(CANARY), "{whole}");
    }
    assert!(
        rows[0].arguments.contains("\"password\""),
        "{}",
        rows[0].arguments
    );
    assert!(
        rows[1].arguments.contains("example.invalid"),
        "{}",
        rows[1].arguments
    );
    let root = built.root.path().display().to_string();
    assert!(
        !rows[2].arguments.contains(&root),
        "the panel's path is the answer's, not the call's"
    );
    assert_eq!(rows[2].arguments, "{}");
}

#[tokio::test]
async fn switched_off_no_call_is_written() {
    let built = Fixture::new().build().await;
    built
        .db
        .set(MCP_AUDIT_LOG_KEY, &false)
        .await
        .expect("stores");
    let client = connect(&built.context).await;
    call(&client, "get_panel_state", json!({})).await;
    call(
        &client,
        "connections_get",
        json!({ "id": 1, "password": CANARY }),
    )
    .await;
    call_unknown(&client).await;
    assert!(rows(&built).await.is_empty());
}

#[tokio::test]
async fn long_arguments_are_cut_and_marked_and_the_code_is_left_out() {
    let built = Fixture::new().build().await;
    let long = json!({ "a": "x".repeat(900), "b": "y".repeat(900), "c": "z".repeat(900),
        "confirmation": "abc" });
    let logged = logged_arguments(&built.context, long.as_object());
    assert_eq!(logged.len(), MCP_AUDIT_ARGUMENTS_MAX_BYTES);
    assert!(logged.ends_with("..."));
    assert!(!logged.contains("confirmation"));
    let hidden = json!({ "path": "a\u{202e}b" });
    let logged = logged_arguments(&built.context, hidden.as_object());
    assert!(logged.contains("\\u{202e}"), "{logged}");
    assert_eq!(logged_arguments(&built.context, None), "{}");
    let _: Value = serde_json::from_str(&logged_arguments(&built.context, None)).expect("json");
}
