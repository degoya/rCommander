use rc_engine::{CommandClass, spec};
use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult};
use serde_json::Value;

use super::PanelParams;
use crate::coverage::command_of;
use crate::test_support::Fixture;

fn body(result: &CallToolResult) -> Value {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

/// Off by default: a panel in a repository knows no marks, and asking starts nothing.
#[tokio::test]
async fn the_status_is_null_while_switched_off() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.root().join(".git")).expect("mkdir");
    let built = fixture.build().await;
    let server = built.server();
    let answer = body(
        &server
            .panel_git_status(Parameters(PanelParams { panel: None }))
            .await
            .expect("answer"),
    );
    assert_eq!(answer["kind"], "git_status", "{answer}");
    assert_eq!(answer["panel"], "left", "{answer}");
    assert!(answer["git"].is_null(), "{answer}");
}

#[test]
fn the_tool_only_reads() {
    let class = command_of("panel_git_status")
        .and_then(spec)
        .map(|spec| spec.class);
    assert_eq!(class, Some(CommandClass::Read));
}
