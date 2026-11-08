//! The button bars' tools over an in-process pipe: the list and one bar answer what the window
//! saved, a run answers or says why not, a run the person must decide is denied, and no tool
//! saves, renames, deletes, imports or opens the window.

use rc_engine::{
    BarItem, BarPlace, BarSpec, ButtonAction, Command, CommandKind, CommandRef, Engine,
    button_bars::BarButton,
};
use rmcp::{RoleClient, service::RunningService};
use serde_json::{Value, json};

use crate::test_support::{Fixture, call};

/// `tool` called, and called again with the code its question answered with.
async fn confirmed(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> Value {
    let asked = call(client, tool, arguments.clone()).await;
    if asked["confirmation_required"] != true {
        return asked;
    }
    let mut again = arguments;
    again["confirmation"] = asked["confirmation"].clone();
    call(client, tool, again).await
}

fn button(action: CommandRef) -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::Command(action),
        icon: None,
        tooltip: "Tip".to_owned(),
    })
}

fn program(command: &str, params: &str) -> CommandRef {
    CommandRef {
        kind: CommandKind::Program,
        command: command.to_owned(),
        params: params.to_owned(),
        ..CommandRef::default()
    }
}

/// Saved the window's way; its id.
async fn bar(engine: &Engine, name: &str, items: Vec<BarItem>) -> i64 {
    let saved = engine
        .dispatch(Command::SaveButtonBar {
            id: None,
            name: name.to_owned(),
            spec: BarSpec {
                items,
                ..BarSpec::default()
            },
        })
        .await
        .expect("the window saves");
    let value = serde_json::to_value(&saved).expect("an outcome serializes");
    value["id"].as_i64().expect("a saved id")
}

#[tokio::test]
async fn the_list_and_one_bar_answer_what_the_window_saved() {
    let built = Fixture::new().build().await;
    let id = bar(
        &built.engine,
        "Tools",
        vec![
            button(program("notepad.exe", "\"%P%N\"")),
            BarItem::Separator,
        ],
    )
    .await;
    let client = built.client().await;

    let listed = call(&client, "button_bars_list", json!({})).await;
    assert_eq!(listed["answer"], "bars", "{listed}");
    assert_eq!(
        listed["bars"][0]["name"], "Default",
        "the first start's bar"
    );
    assert_eq!(listed["bars"][1]["name"], "Tools");

    let one = call(&client, "button_bars_get", json!({ "id": id })).await;
    assert_eq!(one["answer"], "bar", "{one}");
    let items = &one["bar"]["spec"]["items"];
    assert_eq!(items[0]["type"], "button", "{one}");
    assert_eq!(items[0]["action"]["command"]["command"], "notepad.exe");
    assert_eq!(items[0]["tooltip"], "Tip");
    assert_eq!(items[1]["type"], "separator");

    let unknown = call(&client, "button_bars_get", json!({ "id": 4711 })).await;
    assert_eq!(unknown["code"], "button_bar_unknown", "{unknown}");
}

#[tokio::test]
async fn a_run_answers_or_says_why() {
    let built = Fixture::new().with_codes().build().await;
    let id = bar(
        &built.engine,
        "Rows",
        vec![
            BarItem::Separator,
            button(CommandRef {
                kind: CommandKind::Internal,
                command: "panel.refresh".to_owned(),
                ..CommandRef::default()
            }),
        ],
    )
    .await;
    let client = built.client().await;

    let asked = call(
        &client,
        "button_bars_run",
        json!({ "bar": id, "button": 1 }),
    )
    .await;
    assert_eq!(asked["class"], "exec", "a run may start a program: {asked}");
    let ran = confirmed(
        &client,
        "button_bars_run",
        json!({ "bar": id, "button": 1 }),
    )
    .await;
    assert_eq!(ran["answer"], "ran", "{ran}");
    let separator = confirmed(
        &client,
        "button_bars_run",
        json!({ "bar": id, "button": 0 }),
    )
    .await;
    assert_eq!(separator["code"], "button_bar_no_button", "{separator}");
}

/// A button that starts as administrator or asks for its parameters is the person's to run.
#[tokio::test]
async fn an_elevated_or_asking_button_is_denied() {
    let built = Fixture::new().with_codes().build().await;
    let elevated = CommandRef {
        elevated: true,
        ..program("regedit.exe", "")
    };
    let id = bar(
        &built.engine,
        "Admin",
        vec![button(elevated), button(program("notepad.exe", "?%P%N"))],
    )
    .await;
    let client = built.client().await;
    for at in [0, 1] {
        let denied = call(
            &client,
            "button_bars_run",
            json!({ "bar": id, "button": at }),
        )
        .await;
        assert_eq!(denied["code"], "policy_denied", "{at}: {denied}");
    }
}

/// Saving, renaming, deleting, importing and the window are the person's.
#[tokio::test]
async fn the_window_alone_changes_them() {
    let built = Fixture::new().build().await;
    let id = bar(&built.engine, "Mine", Vec::new()).await;
    for command in [
        Command::SaveButtonBar {
            id: Some(id),
            name: "Mine".to_owned(),
            spec: BarSpec::default(),
        },
        Command::RenameButtonBar {
            id,
            name: "Other".to_owned(),
        },
        Command::DeleteButtonBar { id },
        Command::PreviewButtonBarImport {
            path: "/tmp/default.bar".to_owned(),
        },
        Command::ImportButtonBar {
            path: "/tmp/default.bar".to_owned(),
        },
        Command::ButtonBarManager {},
    ] {
        let refused = built
            .engine
            .dispatch_mcp(command)
            .await
            .expect_err("GUI only");
        assert_eq!(refused.code(), "gui_only");
    }
    let focus = built
        .engine
        .dispatch_mcp(Command::FocusButtonBar {
            bar: BarPlace::Horizontal,
        })
        .await;
    assert!(focus.is_ok(), "focus moves no file: {focus:?}");
}

#[test]
fn only_the_list_one_bar_and_the_run_are_tools() {
    let names = crate::RcMcpServer::tool_names();
    for tool in ["button_bars_list", "button_bars_get", "button_bars_run"] {
        assert!(names.contains(&tool.to_owned()), "{tool}");
    }
    for tool in [
        "button_bars_save",
        "button_bars_delete",
        "button_bars_rename",
        "button_bars_import",
        "button_bars_manager",
    ] {
        assert!(!names.contains(&tool.to_owned()), "{tool}");
    }
}

/// The run tool takes no parameters or files of the caller's.
#[test]
fn the_run_tool_takes_no_parameters_or_files() {
    let tools = crate::RcMcpServer::router().list_all();
    let tool = tools
        .iter()
        .find(|tool| tool.name == "button_bars_run")
        .expect("served");
    let properties = tool.input_schema.get("properties").expect("properties");
    assert!(properties.get("params").is_none(), "{properties}");
    assert!(properties.get("files").is_none(), "{properties}");
}
