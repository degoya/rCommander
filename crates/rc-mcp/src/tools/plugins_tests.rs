//! The MCP boundary of the plugins: listing reaches MCP, nothing that decides which code runs
//! does - no tool for it, and the engine refuses the commands from MCP.

use rc_engine::Command;
use rc_plugin_host::test_support::{signing_key, viewer_package};
use serde_json::json;

use crate::{
    RcMcpServer,
    coverage::{COVERAGE, Coverage},
    test_support::{Built, Fixture, call, connect},
};

const ID: &str = "019d0000-0000-7000-8000-0000000000a1";

#[tokio::test]
async fn the_list_reaches_mcp_and_nothing_else_does() {
    let names = RcMcpServer::tool_names();
    let plugin_tools: Vec<&String> = names
        .iter()
        .filter(|name| name.contains("plugin"))
        .collect();
    assert_eq!(plugin_tools, vec!["plugins_list"], "{names:?}");
    for (id, coverage) in COVERAGE.iter().filter(|(id, _)| id.starts_with("plugins.")) {
        assert_eq!(
            matches!(coverage, Coverage::Covered(_)),
            *id == "plugins.list",
            "{id}"
        );
    }

    let Built {
        root,
        engine,
        context,
        ..
    } = Fixture::new().build().await;
    assert!(engine.attach_plugins(root.path().join("data"), None));
    let package = root.path().join("csv.rcplug");
    std::fs::write(
        &package,
        viewer_package(ID, "1.0.0", "author-v1", &signing_key(7), true),
    )
    .expect("write");
    for command in [
        Command::PreviewPlugin {
            path: package.display().to_string(),
        },
        Command::InstallPlugin {
            path: package.display().to_string(),
            fingerprint: None,
            digest: String::new(),
        },
        Command::EnablePlugin { id: ID.to_owned() },
        Command::DisablePlugin { id: ID.to_owned() },
        Command::RollbackPlugin { id: ID.to_owned() },
        Command::ResumePlugin { id: ID.to_owned() },
        Command::RemovePlugin {
            id: ID.to_owned(),
            version: "1.0.0".to_owned(),
        },
    ] {
        let refused = engine.dispatch_mcp(command).await.expect_err("GUI only");
        assert_eq!(refused.code(), "gui_only");
    }

    let client = connect(&context).await;
    let listed = call(&client, "plugins_list", json!({})).await;
    assert_eq!(listed["plugins"], json!([]));
    assert_eq!(listed["plugins_enabled"], json!(true));
    assert_eq!(listed["development_mode"], json!(false));

    // Installed by the window, the plugin is listed with its key and version.
    let fingerprint = rc_plugin_host::key_fingerprint(&signing_key(7).verifying_key());
    engine
        .dispatch(Command::InstallPlugin {
            path: package.display().to_string(),
            fingerprint: Some(fingerprint.clone()),
            digest: rc_plugin_host::sha256_hex(&std::fs::read(&package).expect("read")),
        })
        .await
        .expect("the window installs");
    let listed = call(&client, "plugins_list", json!({ "id": ID })).await;
    let plugin = &listed["plugins"][0];
    assert_eq!(plugin["id"], json!(ID));
    assert_eq!(plugin["version"], json!("1.0.0"));
    assert_eq!(plugin["key_fingerprint"], json!(fingerprint));
    assert_eq!(plugin["permissions"]["extensions"], json!(["csv", "tsv"]));
    assert_eq!(listed["executions"], json!([]));
    let missing = call(&client, "plugins_list", json!({ "id": "nope" })).await;
    assert!(
        missing.to_string().contains("plugin_not_found"),
        "{missing}"
    );
}
