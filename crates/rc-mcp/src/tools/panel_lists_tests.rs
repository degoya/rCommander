use std::fs;

use rc_engine::{CommandClass, PanelId, spec};
use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult};
use serde_json::Value;

use super::{FileFilterParams, FilterKind, ListFileParams, SaveFilterParams, TabRenameParams};
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

#[tokio::test]
async fn a_file_filter_shows_only_programs() {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir(root.join("sub")).expect("mkdir");
    for name in ["a.txt", "setup.exe"] {
        fs::write(root.join(name), b"x").expect("write");
    }
    let built = fixture.build().await;
    let server = built.server();
    let answer = body(
        &server
            .panel_file_filter(Parameters(FileFilterParams {
                kind: FilterKind::Programs,
                mask: None,
                panel: None,
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(answer["entry_count"], 2, "{answer}");
    let names: Vec<String> = built
        .engine
        .panel(PanelId::Left)
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert_eq!(names, ["sub", "setup.exe"]);
    let refused = server
        .panel_file_filter(Parameters(FileFilterParams {
            kind: FilterKind::Mask,
            mask: Some("a\u{1}".into()),
            panel: None,
        }))
        .await
        .expect("answer");
    assert_eq!(body(&refused)["code"], "filter_invalid");
}

#[tokio::test]
async fn a_saved_filter_and_a_tab_name_are_kept() {
    let fixture = Fixture::new();
    let built = fixture.build().await;
    let server = built.server();
    let saved = body(
        &server
            .filters_save(Parameters(SaveFilterParams {
                name: "Text".into(),
                mask: "*.txt".into(),
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(saved["saved"][0]["name"], "Text", "{saved}");
    let tabs = body(
        &server
            .tab_rename(Parameters(TabRenameParams {
                tab: None,
                title: Some("Work".into()),
                panel: None,
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(tabs["tabs"][0]["title"], "Work", "{tabs}");
}

#[tokio::test]
async fn an_existing_selection_file_stays() {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::write(root.join("mark.txt"), b"keep").expect("write");
    let built = fixture.build().await;
    let server = built.server();
    let refused = server
        .panel_selection_to_file(Parameters(ListFileParams {
            path: "mark.txt".into(),
            panel: None,
        }))
        .await
        .expect("answer");
    assert_eq!(body(&refused)["code"], "file_exists");
    assert_eq!(
        fs::read(built.root.path().join("mark.txt")).expect("file"),
        b"keep"
    );
}

#[test]
fn only_the_selection_file_writes() {
    let class = |tool| {
        command_of(tool)
            .and_then(spec)
            .map(|spec| spec.class)
            .expect("a covered tool")
    };
    assert_eq!(class("panel_selection_to_file"), CommandClass::Write);
    for tool in [
        "panel_file_filter",
        "filters_list",
        "filters_save",
        "filters_rename",
        "filters_delete",
        "panel_toggle_ignore_list",
        "panel_save_selection",
        "panel_selection_from_file",
        "panel_selection_from_text",
        "panel_load_list",
        "tab_rename",
        "tab_close_all",
        "history_open_frequent",
    ] {
        assert_eq!(class(tool), CommandClass::Read, "{tool}");
    }
}
