//! The own commands' and the Start menu's tools over an in-process pipe: the lists answer what the
//! window saved, a run answers or says why not, a run the person must decide is denied, and no
//! tool saves, deletes, moves or opens the window.

use rc_engine::{
    Command, CommandKind, CommandRef, Engine, StartItemDraft, StartItemKind, UserCommandDraft,
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

fn program(command: &str, params: &str) -> CommandRef {
    CommandRef {
        kind: CommandKind::Program,
        command: command.to_owned(),
        params: params.to_owned(),
        ..CommandRef::default()
    }
}

fn internal(rows: &str) -> CommandRef {
    CommandRef {
        kind: CommandKind::Internal,
        command: rows.to_owned(),
        ..CommandRef::default()
    }
}

/// Saved the window's way; its id.
async fn own(engine: &Engine, name: &str, action: CommandRef) -> i64 {
    let command = UserCommandDraft {
        name: name.to_owned(),
        action,
        tooltip: format!("Tip of {name}"),
        ..UserCommandDraft::default()
    };
    let saved = engine
        .dispatch(Command::SaveUserCommand { id: None, command })
        .await
        .expect("the window saves");
    saved_id(&saved)
}

/// A Start menu row saved the window's way; its id.
async fn start(engine: &Engine, parent: Option<i64>, item: StartItemDraft) -> i64 {
    let saved = engine
        .dispatch(Command::SaveStartItem {
            id: None,
            parent,
            item,
        })
        .await
        .expect("the window saves");
    saved_id(&saved)
}

fn saved_id(outcome: &rc_engine::Outcome) -> i64 {
    let value = serde_json::to_value(outcome).expect("an outcome serializes");
    value["id"].as_i64().expect("a saved id")
}

fn entry(title: &str, action: CommandRef) -> StartItemDraft {
    StartItemDraft {
        kind: StartItemKind::Item,
        title: title.to_owned(),
        action: Some(action),
    }
}

#[tokio::test]
async fn the_lists_answer_what_the_window_saved() {
    let built = Fixture::new().build().await;
    own(&built.engine, "em_np", program("notepad.exe", "\"%P%N\"")).await;
    own(&built.engine, "em_both", internal("cm_SelectAll,em_np")).await;
    let tools = start(
        &built.engine,
        None,
        StartItemDraft {
            kind: StartItemKind::Submenu,
            title: "Tools".to_owned(),
            action: None,
        },
    )
    .await;
    start(
        &built.engine,
        Some(tools),
        entry("Notepad", program("notepad.exe", "")),
    )
    .await;
    let client = built.client().await;

    let listed = call(&client, "user_commands_list", json!({})).await;
    assert_eq!(listed["kind"], "user_commands", "{listed}");
    assert_eq!(listed["answer"], "commands", "{listed}");
    assert_eq!(
        listed["commands"][0]["name"], "em_both",
        "by name: {listed}"
    );
    assert_eq!(listed["commands"][0]["action"]["kind"], "internal");
    assert_eq!(listed["commands"][1]["name"], "em_np");
    assert_eq!(listed["commands"][1]["action"]["params"], "\"%P%N\"");
    assert_eq!(listed["commands"][1]["tooltip"], "Tip of em_np");

    let menu = call(&client, "start_menu_list", json!({})).await;
    assert_eq!(menu["answer"], "start_menu", "{menu}");
    assert_eq!(menu["items"][0]["kind"], "submenu", "{menu}");
    assert_eq!(menu["items"][0]["title"], "Tools");
    assert_eq!(menu["items"][1]["parent"], tools);
    assert_eq!(menu["items"][1]["depth"], 1);
    assert_eq!(menu["items"][1]["action"]["command"], "notepad.exe");
}

#[tokio::test]
async fn a_run_without_a_program_answers_or_says_why() {
    let built = Fixture::new().with_codes().build().await;
    // A row only the window knows (the command line's history list) does not run over MCP.
    own(&built.engine, "em_history", internal("cmdline_history")).await;
    let client = built.client().await;

    let asked = call(
        &client,
        "user_commands_run",
        json!({ "name": "em_history" }),
    )
    .await;
    assert_eq!(
        asked["class"], "exec",
        "running one may start a program: {asked}"
    );
    let window = confirmed(
        &client,
        "user_commands_run",
        json!({ "name": "em_history" }),
    )
    .await;
    assert_eq!(window["code"], "user_command_window_only", "{window}");

    let unknown = confirmed(
        &client,
        "user_commands_run",
        json!({ "name": "em_nothing", "panel": "left" }),
    )
    .await;
    assert_eq!(unknown["code"], "user_command_unknown", "{unknown}");
    let nowhere = confirmed(&client, "start_menu_run", json!({ "id": 4711 })).await;
    assert_eq!(nowhere["code"], "start_menu_unknown", "{nowhere}");
}

/// A command that starts as administrator or asks for its parameters is the person's to run.
#[tokio::test]
async fn an_elevated_or_asking_command_is_denied() {
    let built = Fixture::new().with_codes().build().await;
    let elevated = CommandRef {
        elevated: true,
        ..program("regedit.exe", "")
    };
    own(&built.engine, "em_reg", elevated.clone()).await;
    own(&built.engine, "em_ask", program("notepad.exe", "?%P%N")).await;
    let entry_id = start(&built.engine, None, entry("Registry", elevated)).await;
    let client = built.client().await;

    for name in ["em_reg", "em_ask", "EM_REG"] {
        let denied = call(&client, "user_commands_run", json!({ "name": name })).await;
        assert_eq!(denied["code"], "policy_denied", "{name}: {denied}");
    }
    let denied = call(&client, "start_menu_run", json!({ "id": entry_id })).await;
    assert_eq!(denied["code"], "policy_denied", "{denied}");
}

/// Saving, deleting, moving and the window are the person's: the engine refuses them over MCP.
#[tokio::test]
async fn the_window_alone_changes_them() {
    let built = Fixture::new().build().await;
    let id = own(&built.engine, "em_np", program("notepad.exe", "")).await;
    let row = start(
        &built.engine,
        None,
        entry("Notepad", program("notepad.exe", "")),
    )
    .await;
    let draft = UserCommandDraft {
        name: "em_other".to_owned(),
        action: program("calc.exe", ""),
        ..UserCommandDraft::default()
    };
    for command in [
        Command::SaveUserCommand {
            id: None,
            command: draft,
        },
        Command::DeleteUserCommand { id },
        Command::UserCommandManager {},
        Command::SaveStartItem {
            id: Some(row),
            parent: None,
            item: entry("Calc", program("calc.exe", "")),
        },
        Command::DeleteStartItem { id: row },
        Command::MoveStartItem {
            id: row,
            parent: None,
            position: Some(0),
        },
        Command::StartMenuManager {},
    ] {
        let refused = built
            .engine
            .dispatch_mcp(command)
            .await
            .expect_err("GUI only");
        assert_eq!(refused.code(), "gui_only");
    }
}

#[test]
fn only_the_lists_and_the_runs_are_tools() {
    let names = crate::RcMcpServer::tool_names();
    for tool in [
        "user_commands_list",
        "start_menu_list",
        "user_commands_run",
        "start_menu_run",
    ] {
        assert!(names.contains(&tool.to_owned()), "{tool}");
    }
    for tool in [
        "user_commands_save",
        "user_commands_delete",
        "user_commands_manager",
        "start_menu_save",
        "start_menu_delete",
        "start_menu_move",
        "start_menu_manager",
    ] {
        assert!(!names.contains(&tool.to_owned()), "{tool}");
    }
}

/// The run tools take no parameters of the caller's: what runs is what the person stored.
#[test]
fn the_run_tools_take_no_parameters() {
    let tools = crate::RcMcpServer::router().list_all();
    for name in ["user_commands_run", "start_menu_run"] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("{name} is served"));
        let properties = tool.input_schema.get("properties").expect("properties");
        assert!(properties.get("params").is_none(), "{name}: {properties}");
    }
}

/// A symbol of rCommander's collection is listed as the window saved it, `rc-symbol:<name>`; one
/// the collection does not have was never saved (RC-110-10, checked by `usercmd.save`).
#[tokio::test]
async fn a_symbol_of_the_collection_is_listed_by_its_reference() {
    let built = Fixture::new().build().await;
    let symbol = UserCommandDraft {
        name: "em_sym".to_owned(),
        action: program("notepad.exe", ""),
        icon_path: "rc-symbol:folder_open".to_owned(),
        ..UserCommandDraft::default()
    };
    built
        .engine
        .dispatch(Command::SaveUserCommand {
            id: None,
            command: symbol,
        })
        .await
        .expect("a symbol of the collection is saved");
    let unknown = UserCommandDraft {
        name: "em_gone".to_owned(),
        action: program("notepad.exe", ""),
        icon_path: "rc-symbol:no_such_symbol".to_owned(),
        ..UserCommandDraft::default()
    };
    let refused = built
        .engine
        .dispatch(Command::SaveUserCommand {
            id: None,
            command: unknown,
        })
        .await
        .expect_err("a name the collection does not have is refused");
    assert_eq!(refused.code(), "user_command_invalid");
    let client = built.client().await;

    let listed = call(&client, "user_commands_list", json!({})).await;
    assert_eq!(
        listed["commands"].as_array().map(Vec::len),
        Some(1),
        "{listed}"
    );
    assert_eq!(listed["commands"][0]["icon_path"], "rc-symbol:folder_open");
}
