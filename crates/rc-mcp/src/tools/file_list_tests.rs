use std::fs;

use rc_engine::{CommandClass, Engine, PanelId, spec};
use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult};
use serde_json::Value;

use super::{
    MaskParams, ModeArg, OpenParams, SelectAction, SelectParams, SetViewParams, ShowHiddenParams,
    SortKeyArg, SortParams,
};
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

/// `sub/`, `a.txt` (3 bytes), `b.md` (1 byte), `.hidden`.
async fn server() -> (tempfile::TempDir, RcMcpServer, Engine) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir(root.join("sub")).expect("mkdir");
    fs::write(root.join("a.txt"), b"aaa").expect("write");
    fs::write(root.join("b.md"), b"b").expect("write");
    fs::write(root.join(".hidden"), b"h").expect("write");
    let built = fixture.build().await;
    let server = built.server();
    (built.root, server, built.engine)
}

fn mask(mask: Option<&str>) -> Parameters<MaskParams> {
    Parameters(MaskParams {
        mask: mask.map(str::to_owned),
        panel: None,
    })
}

/// Num+ and Num- through MCP: files only, unless the mask ends in a backslash.
#[tokio::test]
async fn masks_select_and_deselect_files_as_num_plus_and_num_minus() {
    let (_root, server, _engine) = server().await;
    let selection = |result: &CallToolResult| body(result)["selection"].clone();
    let txt = server
        .panel_select_by_mask(mask(Some("*.txt")))
        .await
        .expect("answer");
    assert_eq!(selection(&txt), serde_json::json!(["a.txt"]));
    let every_file = server
        .panel_select_by_mask(mask(None))
        .await
        .expect("answer");
    assert_eq!(selection(&every_file), serde_json::json!(["a.txt", "b.md"]));
    let dirs = server
        .panel_select_by_mask(mask(Some("*\\")))
        .await
        .expect("answer");
    assert_eq!(
        selection(&dirs),
        serde_json::json!(["a.txt", "b.md", "sub"])
    );
    let fewer = server
        .panel_deselect_by_mask(mask(Some("*.*|*.md")))
        .await
        .expect("answer");
    assert_eq!(selection(&fewer), serde_json::json!(["b.md", "sub"]));
}

#[tokio::test]
async fn panel_select_changes_the_whole_selection() {
    let (_root, server, _engine) = server().await;
    let action = |action| {
        Parameters(SelectParams {
            action,
            panel: None,
        })
    };
    let all = body(
        &server
            .panel_select(action(SelectAction::All))
            .await
            .expect("answer"),
    );
    assert_eq!(
        all["selection"],
        serde_json::json!(["a.txt", "b.md", "sub"])
    );
    let none = body(
        &server
            .panel_select(action(SelectAction::None))
            .await
            .expect("answer"),
    );
    assert_eq!(none["selection"], serde_json::json!([]));
}

#[tokio::test]
async fn sort_view_and_hidden_change_the_panel() {
    let (_root, server, engine) = server().await;
    let sorted = body(
        &server
            .panel_sort(Parameters(SortParams {
                key: SortKeyArg::Size,
                descending: None,
                panel: None,
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(
        sorted["sort"],
        serde_json::json!({ "key": "size", "descending": true })
    );
    let names: Vec<String> = engine
        .panel(PanelId::Left)
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert_eq!(names, ["sub", "a.txt", "b.md"]);

    let brief = body(
        &server
            .panel_set_view(Parameters(SetViewParams {
                mode: ModeArg::Brief,
                panel: None,
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(brief["mode"], "brief");

    let shown = body(
        &server
            .panel_show_hidden(Parameters(ShowHiddenParams { show: Some(true) }))
            .await
            .expect("answer"),
    );
    assert_eq!(
        (shown["show_hidden"].clone(), shown["entry_count"].clone()),
        (Value::Bool(true), Value::from(4))
    );
}

/// A directory only: a file would really be handed to the system here.
#[tokio::test]
async fn file_open_enters_a_directory() {
    let (root, server, _engine) = server().await;
    let inside = body(
        &server
            .file_open(Parameters(OpenParams {
                path: Some("sub".into()),
                panel: None,
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(
        inside["path"]["path"],
        root.path().join("sub").to_string_lossy().as_ref()
    );
}

#[test]
fn file_open_runs_something_and_the_rest_only_reads() {
    let class = |tool: &str| {
        command_of(tool)
            .and_then(spec)
            .map(|spec| spec.class)
            .expect("a covered tool")
    };
    assert_eq!(class("file_open"), CommandClass::Exec);
    for tool in [
        "panel_select",
        "panel_select_by_mask",
        "panel_deselect_by_mask",
        "panel_sort",
        "panel_set_view",
        "panel_show_hidden",
    ] {
        assert_eq!(class(tool), CommandClass::Read, "{tool}");
    }
}
