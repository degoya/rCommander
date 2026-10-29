use rc_engine::{Engine, PanelId};
use rmcp::{
    RoleClient,
    handler::server::wrapper::Parameters,
    model::{CallToolRequestParams, CallToolResult},
    service::RunningService,
};
use serde_json::{Value, json};

use super::{ChangeDriveParams, DrivesListParams};
use crate::test_support::Fixture;
use crate::{RcMcpServer, tools::panels::PanelArg};

fn body(result: &CallToolResult) -> Value {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

async fn server() -> (tempfile::TempDir, RcMcpServer, Engine) {
    let built = Fixture::new().build().await;
    let server = built.server();
    (built.root, server, built.engine)
}

#[tokio::test]
async fn drives_list_answers_the_drive_bar_with_the_root_first() {
    let (_root, server, _engine) = server().await;
    let result = server
        .drives_list(Parameters(DrivesListParams { refresh: false }))
        .await
        .expect("answer");
    let answer = body(&result);
    assert_eq!(answer["kind"], "drives", "{answer}");
    assert_eq!(answer["drives"][0]["id"], "/", "{answer}");
    assert_eq!(answer["drives"][0]["group"], "drives", "{answer}");
}

#[tokio::test]
async fn panel_change_drive_moves_the_panel_and_refuses_an_unknown_drive() {
    let (_root, server, engine) = server().await;
    let moved = server
        .panel_change_drive(Parameters(ChangeDriveParams {
            drive: "/".to_owned(),
            panel: Some(PanelArg::Right),
        }))
        .await
        .expect("answer");
    assert_ne!(moved.is_error, Some(true), "{:?}", body(&moved));
    assert_eq!(engine.panel(PanelId::Right).path.path.to_str(), Some("/"));

    let refused = server
        .panel_change_drive(Parameters(ChangeDriveParams {
            drive: "no such drive".to_owned(),
            panel: None,
        }))
        .await
        .expect("answer");
    assert_eq!(body(&refused)["code"], "drive_not_found");
}

/// An MCP client of a server over a headless engine, confirmation codes allowed.
async fn client() -> (tempfile::TempDir, RunningService<RoleClient, ()>) {
    let built = Fixture::new().with_codes().build().await;
    let client = built.client().await;
    (built.root, client)
}

async fn call(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> Value {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
        .await
        .expect("call");
    body(&result)
}

/// Acceptance: MCP has no tool that maps a network drive (the credential dialog is the
/// person's), and disconnecting one needs the person's confirmation, bound to that drive.
#[tokio::test]
async fn mcp_cannot_map_and_disconnects_only_with_a_confirmation_for_that_drive() {
    let names = RcMcpServer::tool_names();
    assert!(
        names.contains(&"drives_unmap_network".to_owned()),
        "{names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|name| name.contains("map_network") && name != "drives_unmap_network"),
        "{names:?}"
    );

    let (_root, client) = client().await;
    let asked = call(&client, "drives_unmap_network", json!({ "drive": "/" })).await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    assert_eq!(asked["class"], "destructive", "{asked}");
    let code = asked["confirmation"].as_str().expect("code").to_owned();

    // The code was given for "/", not for another drive.
    let other = call(
        &client,
        "drives_unmap_network",
        json!({ "drive": "/mnt/x", "confirmation": code }),
    )
    .await;
    assert_eq!(other["confirmation_required"], true, "{other}");

    // Nor does agreeing to a disconnect agree to forcing it.
    let code = other["confirmation"].as_str().expect("code").to_owned();
    let forced = call(
        &client,
        "drives_unmap_network",
        json!({ "drive": "/mnt/x", "force": true, "confirmation": code }),
    )
    .await;
    assert_eq!(forced["confirmation_required"], true, "{forced}");

    let code = forced["confirmation"].as_str().expect("code").to_owned();
    let done = call(
        &client,
        "drives_unmap_network",
        json!({ "drive": "/mnt/x", "force": true, "confirmation": code }),
    )
    .await;
    // Confirmed, the engine decides: no such drive here.
    assert_eq!(done["code"], "drive_not_found", "{done}");
}
