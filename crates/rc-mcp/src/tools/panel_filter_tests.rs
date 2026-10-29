use std::fs;

use rc_engine::{CommandClass, Engine, PanelId, spec};
use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult};
use serde_json::Value;

use super::FilterParams;
use crate::test_support::Fixture;
use crate::{RcMcpServer, coverage::command_of};

fn body(result: &CallToolResult) -> Value {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

/// `sub/`, `a.txt`, `b.md`, `notes.txt`.
async fn server() -> (tempfile::TempDir, RcMcpServer, Engine) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir(root.join("sub")).expect("mkdir");
    for name in ["a.txt", "b.md", "notes.txt"] {
        fs::write(root.join(name), b"x").expect("write");
    }
    let built = fixture.build().await;
    let server = built.server();
    (built.root, server, built.engine)
}

fn pattern(pattern: Option<&str>) -> Parameters<FilterParams> {
    Parameters(FilterParams {
        pattern: pattern.map(str::to_owned),
        panel: None,
    })
}

#[tokio::test]
async fn the_filter_narrows_the_panel_and_lifts_again() {
    let (_root, server, engine) = server().await;
    let txt = body(
        &server
            .panel_filter(pattern(Some("*.txt")))
            .await
            .expect("answer"),
    );
    assert_eq!(txt["filter"], "*.txt");
    assert_eq!(txt["entry_count"], 2);
    let names: Vec<String> = engine
        .panel(PanelId::Left)
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert_eq!(names, ["a.txt", "notes.txt"]);

    let all = body(&server.panel_filter(pattern(None)).await.expect("answer"));
    assert!(all.get("filter").is_none(), "{all}");
    assert_eq!(all["entry_count"], 4);
}

#[tokio::test]
async fn a_filter_with_a_line_break_is_refused() {
    let (_root, server, _engine) = server().await;
    let refused = server
        .panel_filter(pattern(Some("a\nb")))
        .await
        .expect("answer");
    assert_eq!(refused.is_error, Some(true));
    assert_eq!(body(&refused)["code"], "filter_invalid");
}

#[test]
fn the_filter_only_reads_and_the_command_line_is_no_tool() {
    let class = command_of("panel_filter")
        .and_then(spec)
        .map(|spec| spec.class)
        .expect("a covered tool");
    assert_eq!(class, CommandClass::Read);
    assert_eq!(
        spec("cmdline.run").map(|spec| spec.class),
        Some(CommandClass::Exec)
    );
    assert!(
        !RcMcpServer::tool_names()
            .iter()
            .any(|name| name.contains("cmdline"))
    );
}
