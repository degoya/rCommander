//! The settings tools over an in-process pipe: what an agent may change, and what it may not.

use rc_db::Db;
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::test_support::Fixture;
use crate::{McpToken, POLICY_KEY, Policy, TOKEN_KEY, config};

struct Setup {
    _dir: TempDir,
    db: Db,
    client: RunningService<RoleClient, ()>,
}

async fn setup() -> Setup {
    let built = Fixture::new().build().await;
    let client = built.client().await;
    Setup {
        _dir: built.root,
        db: built.db,
        client,
    }
}

async fn call(setup: &Setup, tool: &str, arguments: Value) -> Value {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = setup
        .client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
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

#[tokio::test]
async fn settings_read_and_change_without_the_token() {
    let setup = setup().await;
    let token = McpToken::load_or_create(&setup.db).await.expect("token");

    let all = call(&setup, "settings_get", json!({})).await;
    let rendered = all.to_string();
    assert!(!rendered.contains(token.expose()), "no token: {rendered}");
    assert!(!rendered.contains(TOKEN_KEY), "no token row: {rendered}");
    assert!(rendered.contains("appearance.theme"), "{rendered}");

    let changed = call(
        &setup,
        "settings_set",
        json!({ "key": "appearance.theme", "value": "dark" }),
    )
    .await;
    assert_eq!(changed["settings"][0]["value"], "dark", "{changed}");
    let one = call(&setup, "settings_get", json!({ "key": "appearance.theme" })).await;
    assert_eq!(one["settings"][0]["value"], "dark", "{one}");

    let invalid = call(
        &setup,
        "settings_set",
        json!({ "key": "appearance.theme", "value": "neon" }),
    )
    .await;
    assert_eq!(invalid["code"], "setting_invalid", "{invalid}");
}

/// The acceptance check: `settings_set` changes neither the MCP token nor the MCP rights (nor
/// the switch, the port or the vault's idle lock).
#[tokio::test]
async fn settings_set_cannot_change_the_token_or_the_rights() {
    let setup = setup().await;
    let token = McpToken::load_or_create(&setup.db).await.expect("token");
    let attempts = [
        (TOKEN_KEY, json!("attacker-chosen-token")),
        (
            POLICY_KEY,
            json!({ "read": "allowed", "write": "allowed", "destructive": "allowed", "exec": "allowed" }),
        ),
        ("mcp.policy.exec", json!("allowed")),
        (config::ENABLED_KEY, json!(false)),
        (config::PORT_KEY, json!(9999)),
        ("vault.idle_lock_minutes", json!(0)),
        ("vault.max_unlocked_hours", json!(0)),
        ("vault.lock_on_session_lock", json!(false)),
        ("vault.wincred_source", json!(false)),
        ("vault.shared_credentials", json!(false)),
    ];
    for (key, value) in attempts {
        let refused = call(
            &setup,
            "settings_set",
            json!({ "key": key, "value": value }),
        )
        .await;
        assert_eq!(refused["code"], "setting_protected", "{key}: {refused}");
    }
    let kept = McpToken::load_or_create(&setup.db).await.expect("token");
    assert_eq!(kept, token, "the token is unchanged");
    assert_eq!(
        Policy::load(&setup.db).await.expect("policy"),
        Policy::default(),
        "the rights are unchanged"
    );
    assert_eq!(
        setup.db.get::<Value>(config::PORT_KEY).await.expect("read"),
        None
    );
}

/// RC-080-17: what F10 does, the question before quitting and deleting for good where there is
/// no recycle bin are an agent's to read and switch.
#[tokio::test]
async fn settings_read_and_switch_f10_the_quit_question_and_the_permanent_delete() {
    let setup = setup().await;
    let f10 = call(&setup, "settings_get", json!({ "key": "keys.f10" })).await;
    assert_eq!(f10["settings"][0]["value"], "close_and_quit", "{f10}");
    for (key, value) in [
        ("keys.f10", json!("close_dialogs")),
        ("keys.f10", json!("off")),
        ("app.confirm_quit_with_transfers", json!(false)),
        ("delete.permanent_without_trash", json!(true)),
    ] {
        let changed = call(
            &setup,
            "settings_set",
            json!({ "key": key, "value": value }),
        )
        .await;
        assert_eq!(changed["settings"][0]["value"], value, "{key}: {changed}");
    }
    let invalid = call(
        &setup,
        "settings_set",
        json!({ "key": "keys.f10", "value": "quit_only" }),
    )
    .await;
    assert_eq!(invalid["code"], "setting_invalid", "{invalid}");
}

/// RC-100-21: an agent reads the keys in force, the person's changes marked, and moves none.
#[tokio::test]
async fn keymap_list_reads_the_keys_and_no_tool_moves_one() {
    let setup = setup().await;
    let keys = call(&setup, "keymap_list", json!({})).await;
    let rows = keys["rows"].as_array().expect("rows");
    let copy = rows
        .iter()
        .find(|row| row["name"] == "copy")
        .expect("the copy");
    assert_eq!(copy["keys"], "F5", "{copy}");
    assert_eq!(copy["changed"], false, "{copy}");
    let tab = rows.iter().find(|row| row["keys"] == "Tab").expect("Tab");
    assert_eq!(tab["locked"], "focus", "{tab}");

    let refused = call(
        &setup,
        "settings_set",
        json!({ "key": "keys.overrides", "value": "F5= Ctrl+K=copy" }),
    )
    .await;
    assert_eq!(refused["code"], "setting_protected", "{refused}");
    let tools = setup.client.list_all_tools().await.expect("tools");
    assert!(
        !tools.iter().any(|tool| tool.name.contains("keymap_set")),
        "no tool moves a key"
    );
    let again = call(&setup, "keymap_list", json!({})).await;
    assert_eq!(again, keys, "nothing moved");
}
