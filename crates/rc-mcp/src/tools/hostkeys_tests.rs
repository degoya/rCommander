//! The MCP boundary of host-key trust: keys can be listed and, with a code, forgotten - never
//! trusted, accepted or replaced.

use rc_engine::{Command, HostKeyAnswer, OfferedKey};
use rmcp::model::CallToolRequestParams;
use serde_json::json;
use tempfile::TempDir;

use crate::test_support::{Fixture, call, connect};
use crate::{
    McpContext,
    coverage::{COVERAGE, Coverage},
};

const ED25519: &str = include_str!("../../../rc-remote/tests/fixtures/known_hosts/ed25519.pub");
const OTHER: &str = include_str!("../../../rc-remote/tests/fixtures/known_hosts/other.pub");

fn key(pub_line: &str) -> OfferedKey {
    let mut fields = pub_line.split_whitespace();
    OfferedKey::from_openssh(fields.next().expect("type"), fields.next().expect("key"))
        .expect("a valid key")
}

async fn setup() -> (TempDir, McpContext) {
    // Headless: these tests drive the confirmation-code path (RC-010-13).
    let built = Fixture::new().with_codes().build().await;
    (built.root, built.context)
}

/// A trusted key, through the engine as the GUI would store it.
async fn trusted(context: &McpContext, host: &str, pub_line: &str) {
    let engine = context.engine();
    let mut events = engine.subscribe();
    let _ = engine.check_host_key(host, 22, key(pub_line)).await;
    let question = loop {
        if let rc_engine::EngineEvent::HostKeyQuestion { question } =
            events.try_recv().expect("an event")
        {
            break *question;
        }
    };
    engine
        .dispatch(Command::AnswerHostKey {
            question: question.id,
            answer: HostKeyAnswer::TrustAndSave,
        })
        .await
        .expect("trusted");
}

#[tokio::test]
async fn no_tool_trusts_accepts_replaces_or_imports_a_key() {
    let (_root, context) = setup().await;
    let client = connect(&context).await;
    let tools = client.list_all_tools().await.expect("tools");
    let mut names: Vec<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
    names.sort();
    let host_tools: Vec<&String> = names
        .iter()
        .filter(|name| name.contains("host") || name.contains("key"))
        .collect();
    // `keymap_list` reads the keyboard shortcuts (RC-100-21), not a host key; it changes nothing.
    assert_eq!(
        host_tools,
        vec!["hostkeys_list", "keymap_list"],
        "{names:?}"
    );
    // `connections_import_ssh_config`, `import_total_commander` and
    // `import_remote_desktop_manager` import connections, never a host key.
    let imports = [
        "connections_import_ssh_config",
        "import_total_commander",
        "import_remote_desktop_manager",
    ];
    for word in ["trust", "accept", "answer", "replace", "import", "confirm"] {
        assert!(
            names
                .iter()
                .filter(|name| !imports.contains(&name.as_str()))
                .all(|name| !name.contains(word)),
            "{word}: {names:?}"
        );
    }
    for id in [
        "hostkeys.answer",
        "hostkeys.replace",
        "hostkeys.remove",
        "hostkeys.import_known_hosts",
    ] {
        assert!(
            COVERAGE
                .iter()
                .any(|(row, coverage)| *row == id && matches!(coverage, Coverage::Omitted(_))),
            "{id} has no MCP tool"
        );
    }
}

#[tokio::test]
async fn a_call_cannot_accept_a_pending_key() {
    let (_root, context) = setup().await;
    let engine = context.engine().clone();
    let mut events = engine.subscribe();
    let error = engine
        .check_host_key("box.example", 22, key(ED25519))
        .await
        .expect_err("unknown");
    assert_eq!(error.code(), "host_key_unknown");
    let question = loop {
        if let rc_engine::EngineEvent::HostKeyQuestion { question } =
            events.try_recv().expect("an event")
        {
            break *question;
        }
    };

    let client = connect(&context).await;
    for tool in ["hostkeys_answer", "hostkeys_trust", "hostkeys_replace"] {
        let arguments = json!({ "question": question.id, "answer": "trust_and_save" });
        let refused = client
            .call_tool(
                CallToolRequestParams::new(tool.to_owned())
                    .with_arguments(arguments.as_object().cloned().unwrap_or_default()),
            )
            .await;
        assert!(refused.is_err(), "{tool} is no tool: {refused:?}");
    }
    let listed = call(&client, "hostkeys_list", json!({ "filter": "box.example" })).await;
    assert_eq!(listed["keys"], json!([]), "{listed}");
    assert_eq!(engine.host_key_questions(), vec![question]);
    let still = engine
        .check_host_key("box.example", 22, key(ED25519))
        .await
        .expect_err("still unknown");
    assert_eq!(still.code(), "host_key_unknown");
}

#[tokio::test]
async fn the_list_shows_fingerprints_but_no_key_blobs() {
    let (_root, context) = setup().await;
    trusted(&context, "box.example", ED25519).await;
    let client = connect(&context).await;
    let listed = call(&client, "hostkeys_list", json!({})).await;
    let keys = listed["keys"].as_array().expect("keys");
    assert_eq!(keys.len(), 1, "{listed}");
    assert_eq!(keys[0]["host"], "box.example");
    assert_eq!(keys[0]["port"], 22);
    assert_eq!(keys[0]["algorithm"], "ssh-ed25519");
    assert_eq!(keys[0]["fingerprint"], key(ED25519).fingerprint.as_str());
    assert_eq!(keys[0]["source"], "user");
    assert!(keys[0].get("public_key").is_none(), "{listed}");
}

/// RC-080-12 (X7): forgetting a key is a person's decision, as removing a certificate is - an
/// agent that forgot a changed key would turn the stop into a first sighting.
#[tokio::test]
async fn no_tool_forgets_a_key() {
    let (_root, context) = setup().await;
    trusted(&context, "box.example", ED25519).await;
    trusted(&context, "other.example", OTHER).await;
    let client = connect(&context).await;
    let listed = call(&client, "hostkeys_list", json!({ "filter": "box" })).await;
    let id = listed["keys"][0]["id"].as_i64().expect("id");
    let refused = client
        .call_tool(
            CallToolRequestParams::new("hostkeys_remove".to_owned())
                .with_arguments(json!({ "id": id }).as_object().cloned().unwrap_or_default()),
        )
        .await;
    assert!(refused.is_err(), "hostkeys_remove is no tool: {refused:?}");
    let refused = context
        .engine()
        .dispatch_mcp(Command::RemoveHostKey { id })
        .await
        .expect_err("a person decides");
    assert_eq!(refused.code(), "gui_only");
    let still = call(&client, "hostkeys_list", json!({})).await;
    assert_eq!(still["keys"].as_array().map(Vec::len), Some(2), "{still}");
}
