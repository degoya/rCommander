//! The own commands through the commands: saved, listed, renamed and deleted, every boundary
//! checked, every change announced.

use crate::{
    Command, CommandKind, CommandRef, Engine, EngineError, EngineEvent, Outcome, UserCommandDraft,
    UserCommandsOutcome, test_support,
};

fn outcome(result: Result<Outcome, EngineError>) -> UserCommandsOutcome {
    match result.expect("answered") {
        Outcome::UserCommands(outcome) => *outcome,
        other => panic!("a user-commands outcome, got {other:?}"),
    }
}

fn draft(name: &str, command: &str) -> UserCommandDraft {
    UserCommandDraft {
        name: name.into(),
        action: CommandRef {
            command: command.into(),
            params: "\"%P%N\"".into(),
            ..CommandRef::default()
        },
        tooltip: " Notepad ".into(),
        ..UserCommandDraft::default()
    }
}

async fn save(engine: &Engine, id: Option<i64>, draft: UserCommandDraft) -> Result<i64, String> {
    match engine
        .dispatch(Command::SaveUserCommand { id, command: draft })
        .await
    {
        Ok(Outcome::UserCommands(outcome)) => match *outcome {
            UserCommandsOutcome::Saved { id } => Ok(id),
            other => panic!("saved, got {other:?}"),
        },
        Ok(other) => panic!("a user-commands outcome, got {other:?}"),
        Err(error) => Err(error.code().to_owned()),
    }
}

#[tokio::test]
async fn an_own_command_is_saved_listed_changed_and_deleted() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let id = save(&engine, None, draft("EM_NP", "notepad.exe"))
        .await
        .expect("saved");
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::UserCommandsChanged)
    );
    let UserCommandsOutcome::Commands { commands } =
        outcome(engine.dispatch(Command::ListUserCommands {}).await)
    else {
        panic!("a list");
    };
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].id, id);
    assert_eq!(commands[0].name, "em_np", "kept in lower case");
    assert_eq!(commands[0].tooltip, "Notepad");
    assert_eq!(commands[0].action.command, "notepad.exe");

    assert_eq!(
        save(&engine, Some(id), draft("em_editor", "code"))
            .await
            .expect("changed"),
        id
    );
    assert_eq!(
        save(&engine, None, draft("em_Editor", "x")).await,
        Err("user_command_name_taken".to_owned())
    );
    let deleted = outcome(engine.dispatch(Command::DeleteUserCommand { id }).await);
    assert_eq!(deleted, UserCommandsOutcome::Deleted { removed: 1 });
    assert_eq!(
        engine
            .dispatch(Command::DeleteUserCommand { id })
            .await
            .expect_err("gone")
            .code(),
        "user_command_unknown"
    );
}

#[tokio::test]
async fn what_does_not_fit_is_refused() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let long = format!("em_{}", "x".repeat(41));
    for name in ["np", "em_", "em_n p", "em_\u{e4}", long.as_str()] {
        assert_eq!(
            save(&engine, None, draft(name, "notepad.exe")).await,
            Err("user_command_name".to_owned()),
            "{name}"
        );
    }
    assert_eq!(
        save(&engine, None, draft("em_empty", "  ")).await,
        Err("user_command_invalid".to_owned())
    );
    let mut many = draft("em_many", "");
    many.action = CommandRef {
        kind: CommandKind::Internal,
        command: vec!["panel.refresh"; 17].join(","),
        ..CommandRef::default()
    };
    assert_eq!(
        save(&engine, None, many).await,
        Err("user_command_invalid".to_owned()),
        "at most 16 rows"
    );
    let mut elevated_rows = draft("em_rows", "");
    elevated_rows.action = CommandRef {
        kind: CommandKind::Internal,
        command: "copy".into(),
        elevated: true,
        ..CommandRef::default()
    };
    assert_eq!(
        save(&engine, None, elevated_rows).await,
        Err("user_command_invalid".to_owned()),
        "only a program starts as administrator"
    );
    let mut control = draft("em_ctl", "notepad.exe");
    control.action.params = "a\nb".into();
    assert_eq!(
        save(&engine, None, control).await,
        Err("user_command_invalid".to_owned())
    );
}

/// The rows of an internal command are trimmed and kept; an own command is named in lower case.
#[tokio::test]
async fn rows_and_own_commands_are_kept_cleaned() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let mut rows = draft("em_rows", "");
    rows.action = CommandRef {
        kind: CommandKind::Internal,
        command: " copy , cm_List ,, em_Other ".into(),
        params: "dropped".into(),
        ..CommandRef::default()
    };
    save(&engine, None, rows).await.expect("saved");
    let mut own = draft("em_own", "");
    own.action = CommandRef {
        kind: CommandKind::Own,
        command: " EM_ROWS ".into(),
        ..CommandRef::default()
    };
    save(&engine, None, own).await.expect("saved");
    let commands = engine.user_commands().await.expect("listed");
    let kept: Vec<(&str, &str, &str)> = commands
        .iter()
        .map(|command| {
            (
                command.name.as_str(),
                command.action.command.as_str(),
                command.action.params.as_str(),
            )
        })
        .collect();
    assert_eq!(
        kept,
        [
            ("em_own", "em_rows", ""),
            ("em_rows", "copy,cm_List,em_Other", ""),
        ]
    );
}

#[tokio::test]
async fn the_window_is_asked_for_by_an_event() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::UserCommandManager {})
        .await
        .expect("answered");
    engine
        .dispatch(Command::StartMenuManager {})
        .await
        .expect("answered");
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::UserCommandsWindow { start_menu: false })
    );
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::UserCommandsWindow { start_menu: true })
    );
}

/// A symbol of the collection is saved by its name; a name the collection does not have is
/// refused (RC-110-10), a file stays a file.
#[tokio::test]
async fn a_symbol_of_the_collection_is_checked_on_saving() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let mut known = draft("em_sym", "notepad.exe");
    known.icon_path = " rc-symbol:folder_open ".into();
    let id = save(&engine, None, known).await.expect("saved");
    let UserCommandsOutcome::Commands { commands } =
        outcome(engine.dispatch(Command::ListUserCommands {}).await)
    else {
        panic!("a list");
    };
    let stored = commands.iter().find(|command| command.id == id);
    assert_eq!(
        stored.map(|command| command.icon_path.as_str()),
        Some("rc-symbol:folder_open")
    );
    let mut unknown = draft("em_gone", "notepad.exe");
    unknown.icon_path = "rc-symbol:no_such_symbol".into();
    assert_eq!(
        save(&engine, None, unknown).await,
        Err("user_command_invalid".to_owned())
    );
    let mut file = draft("em_file", "notepad.exe");
    file.icon_path = "%COMMANDER_PATH%\\WCMICONS.DLL".into();
    file.icon_index = 5;
    assert!(
        save(&engine, None, file).await.is_ok(),
        "a file is no reference"
    );
}
