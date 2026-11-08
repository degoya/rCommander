use std::fs;

use rc_engine::{CommandClass, Engine, ListMode, PanelId, spec};
use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult};
use serde_json::Value;

use super::{
    MaskParams, ModeArg, OpenParams, SelectAction, SelectParams, SetViewParams, ShowHiddenParams,
    ShowTreeParams, SortKeyArg, SortParams,
};
use crate::test_support::{Fixture, hidden_file};
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

/// `sub/`, `a.txt` (3 bytes), `b.md` (1 byte), `.hidden` (hidden on every platform).
async fn server() -> (tempfile::TempDir, RcMcpServer, Engine) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir(root.join("sub")).expect("mkdir");
    fs::write(root.join("a.txt"), b"aaa").expect("write");
    fs::write(root.join("b.md"), b"b").expect("write");
    hidden_file(&root.join(".hidden"), "h");
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
                set: None,
                panel: None,
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(brief["mode"], "brief");

    // The thumbnails (RC-100-89): the same entries, in the window's grid.
    let thumbnails = body(
        &server
            .panel_set_view(Parameters(SetViewParams {
                mode: ModeArg::Thumbnails,
                set: None,
                panel: None,
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(thumbnails["mode"], "thumbnails");
    assert_eq!(engine.panel(PanelId::Left).mode, ListMode::Thumbnails);

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

/// Ctrl+F8 through MCP (RC-100-88): the tree in the panel's place and back; the folder stays.
#[tokio::test]
async fn panel_show_tree_switches_between_the_tree_and_the_files() {
    let (root, server, _engine) = server().await;
    let show = |show| {
        Parameters(ShowTreeParams {
            show,
            panel: Some(super::super::panels::PanelArg::Right),
        })
    };
    let tree = body(&server.panel_show_tree(show(None)).await.expect("answer"));
    assert_eq!(
        (tree["view"].clone(), tree["panel"].clone()),
        ("tree".into(), "right".into())
    );
    assert_eq!(
        tree["path"]["path"],
        root.path().to_string_lossy().as_ref(),
        "the panel keeps its folder"
    );
    let files = body(
        &server
            .panel_show_tree(show(Some(false)))
            .await
            .expect("answer"),
    );
    assert_eq!(files["view"], "files");
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
        "panel_show_tree",
    ] {
        assert_eq!(class(tool), CommandClass::Read, "{tool}");
    }
}

/// RC-100-91: `file_open` follows the internal associations as Enter does; a file whose rule
/// starts as administrator or asks for its parameters is the person's to open - denied here.
/// The rules themselves are read with `settings_get` and never set over MCP.
#[tokio::test]
async fn file_open_with_an_elevated_rule_is_the_persons() {
    use rc_engine::{
        CommandRef, FileAssociation, associations::write_rules, settings::FILE_ASSOCIATIONS_KEY,
    };
    use serde_json::json;

    use crate::test_support::call;

    let fixture = Fixture::new();
    fs::write(fixture.root().join("run.md"), b"x").expect("write");
    let built = fixture.build().await;
    let rule = |elevated: bool, params: &str| FileAssociation {
        mask: "*.md".into(),
        open: Some(CommandRef {
            command: "md-viewer.exe".into(),
            params: params.into(),
            elevated,
            ..CommandRef::default()
        }),
        ..FileAssociation::default()
    };
    let client = built.client().await;
    for stored in [rule(true, "%1"), rule(false, "?%1")] {
        built
            .engine
            .set_protected_setting(FILE_ASSOCIATIONS_KEY, &json!(write_rules(&[stored])))
            .await
            .expect("the window stores");
        let denied = call(&client, "file_open", json!({ "path": "run.md" })).await;
        assert_eq!(denied["code"], "policy_denied", "{denied}");
    }
    let read = call(
        &client,
        "settings_get",
        json!({ "key": FILE_ASSOCIATIONS_KEY }),
    )
    .await;
    assert!(read.to_string().contains("md-viewer.exe"), "{read}");
    let refused = call(
        &client,
        "settings_set",
        json!({ "key": FILE_ASSOCIATIONS_KEY, "value": "" }),
    )
    .await;
    assert_eq!(refused["code"], "setting_protected", "{refused}");
}
