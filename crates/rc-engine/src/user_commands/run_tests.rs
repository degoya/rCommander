//! Own commands and Start menu entries run through the commands, their programs recorded in
//! `launch::STARTED` instead of started (one process per test under nextest; each test looks
//! for its own program's name all the same).

use tokio::sync::broadcast::error::TryRecvError;

use super::super::launch::{Launch, STARTED};
use crate::{
    AskSource, Command, CommandKind, CommandRef, Engine, EngineError, EngineEvent, Outcome,
    PanelId, StartItemDraft, StartItemKind, UserCommandDraft, UserCommandsOutcome,
    recover::LockOrRecover as _, test_support,
};

fn program(command: &str, params: &str) -> CommandRef {
    CommandRef {
        command: command.into(),
        params: params.into(),
        ..CommandRef::default()
    }
}

fn rows(rows: &str) -> CommandRef {
    CommandRef {
        kind: CommandKind::Internal,
        command: rows.into(),
        ..CommandRef::default()
    }
}

fn own(name: &str) -> CommandRef {
    CommandRef {
        kind: CommandKind::Own,
        command: name.into(),
        ..CommandRef::default()
    }
}

async fn save(engine: &Engine, name: &str, action: CommandRef) {
    engine
        .dispatch(Command::SaveUserCommand {
            id: None,
            command: UserCommandDraft {
                name: name.into(),
                action,
                ..UserCommandDraft::default()
            },
        })
        .await
        .expect("saved");
}

async fn run(engine: &Engine, name: &str) -> Result<Outcome, EngineError> {
    engine
        .dispatch(Command::RunUserCommand {
            name: name.into(),
            panel: Some(PanelId::Left),
            params: None,
        })
        .await
}

fn ran(result: Result<Outcome, EngineError>) -> (usize, bool) {
    match result.expect("ran") {
        Outcome::UserCommands(outcome) => match *outcome {
            UserCommandsOutcome::Ran { started, asking } => (started, asking),
            other => panic!("a run, got {other:?}"),
        },
        other => panic!("a user-commands outcome, got {other:?}"),
    }
}

/// The launches of `program` recorded so far.
fn started(program: &str) -> Vec<Launch> {
    STARTED
        .lock_or_recover("test")
        .iter()
        .filter(|launch| launch.program == program)
        .cloned()
        .collect()
}

/// Marks `names` in the left panel and puts the cursor on `cursor`.
fn mark(engine: &Engine, names: &[&str], cursor: &str) {
    let mut state = engine.state();
    let panel = state.panel_mut(PanelId::Left);
    for name in names {
        panel.selection.insert((*name).to_owned());
    }
    panel.cursor = panel
        .entries
        .iter()
        .position(|entry| entry.name == cursor)
        .expect("the cursor's entry");
}

#[tokio::test]
async fn an_own_command_starts_its_program_with_the_panels_values() {
    let (root, engine) = test_support::engine_with_db(&[("a b.txt", "x"), ("c.txt", "y")]).await;
    save(&engine, "em_np", program("np-test", "\"%P%N\"")).await;
    mark(&engine, &[], "a b.txt");
    assert_eq!(ran(run(&engine, "EM_NP").await), (1, false));
    let launch = started("np-test").pop().expect("started");
    let dir = root.path().to_string_lossy().into_owned();
    let path = format!("{dir}{}a b.txt", std::path::MAIN_SEPARATOR);
    if cfg!(windows) {
        assert_eq!(launch.args, [format!("\"{path}\"")]);
    } else {
        assert_eq!(launch.args, [path]);
    }
    assert_eq!(
        launch.folder.as_deref(),
        Some(root.path()),
        "the panel's folder"
    );
    assert!(!launch.elevated);
}

/// `%S` with three marked names with blanks: quoted on Windows, one argument each elsewhere.
#[tokio::test]
async fn three_marked_names_with_blanks_reach_the_program() {
    let (_root, engine) =
        test_support::engine_with_db(&[("a 1.txt", ""), ("b 2.txt", ""), ("c 3.txt", "")]).await;
    save(&engine, "em_three", program("three-test", "-x %S")).await;
    mark(&engine, &["a 1.txt", "b 2.txt", "c 3.txt"], "a 1.txt");
    ran(run(&engine, "em_three").await);
    let launch = started("three-test").pop().expect("started");
    if cfg!(windows) {
        assert_eq!(launch.args, ["-x \"a 1.txt\" \"b 2.txt\" \"c 3.txt\""]);
    } else {
        assert_eq!(launch.args, ["-x", "a 1.txt", "b 2.txt", "c 3.txt"]);
    }
}

#[tokio::test]
async fn em_recursion_is_refused() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    save(&engine, "em_self", rows("em_self")).await;
    save(&engine, "em_ping", own("em_pong")).await;
    save(&engine, "em_pong", rows("panel.refresh,em_ping")).await;
    for name in ["em_self", "em_ping"] {
        let error = run(&engine, name).await.expect_err("refused");
        assert_eq!(error.code(), "user_command_recursion", "{name}");
    }
    // Eight levels deep is still allowed.
    for level in 0..8 {
        save(
            &engine,
            &format!("em_l{level}"),
            own(&format!("em_l{}", level + 1)),
        )
        .await;
    }
    save(&engine, "em_l8", program("deep-test", "")).await;
    ran(run(&engine, "em_l0").await);
    assert_eq!(started("deep-test").len(), 1);
}

/// `?` in front of the parameters hands them to the window; it runs the command again with
/// them, and only then does the program start.
#[tokio::test]
async fn a_question_mark_asks_before_the_start() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    save(&engine, "em_ask", program("ask-test", "?-n %N")).await;
    let mut events = engine.subscribe();
    assert_eq!(ran(run(&engine, "em_ask").await), (0, true));
    assert!(started("ask-test").is_empty());
    let request = loop {
        match events.try_recv() {
            Ok(EngineEvent::UserCommandAsk(request)) => break request,
            Ok(_) => {}
            Err(TryRecvError::Empty | TryRecvError::Closed) => panic!("no question"),
            Err(TryRecvError::Lagged(_)) => {}
        }
    };
    assert_eq!(
        request.source,
        AskSource::Own {
            name: "em_ask".into()
        }
    );
    assert_eq!(request.params, "-n %N");
    let answered = engine
        .dispatch(Command::RunUserCommand {
            name: "em_ask".into(),
            panel: Some(request.panel),
            params: Some("-m %N".into()),
        })
        .await;
    assert_eq!(ran(answered), (1, false));
    let launch = started("ask-test").pop().expect("started");
    assert_eq!(launch.args.join(" "), "-m a.txt");
}

#[tokio::test]
async fn internal_rows_run_through_the_registry_or_the_window() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    save(
        &engine,
        "em_rows",
        rows("panel.refresh,cm_SrcFull,cm_AddPathToCmdline"),
    )
    .await;
    let mut events = engine.subscribe();
    assert_eq!(ran(run(&engine, "em_rows").await), (0, false));
    let mut window_rows = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let EngineEvent::WindowRow { name } = event {
            window_rows.push(name);
        }
    }
    assert_eq!(window_rows, ["cmdline_insert_folder"]);
    save(&engine, "em_unknown", rows("panel.refresh,cm_NoSuchThing")).await;
    assert_eq!(
        run(&engine, "em_unknown")
            .await
            .expect_err("refused")
            .code(),
        "user_command_unknown_row"
    );
    assert_eq!(
        run(&engine, "em_missing")
            .await
            .expect_err("refused")
            .code(),
        "user_command_unknown"
    );
}

/// RC-110-15: an own command with Total Commander's `cm_VisButtonBar` - in an imported
/// `usercmd.ini` or a button - shows and hides the button bar, though no key runs it.
#[tokio::test]
async fn cm_vis_button_bar_toggles_the_button_bar() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let db = engine.db().expect("a database").clone();
    let key = crate::BarKind::ButtonBar.setting_key();
    assert!(crate::settings::stored_bool(&db, key).await);
    save(&engine, "em_bar", rows("cm_VisButtonBar")).await;
    run(&engine, "em_bar").await.expect("ran");
    assert!(!crate::settings::stored_bool(&db, key).await);
    save(&engine, "em_bar2", rows("cm_VisButtonBar2")).await;
    run(&engine, "em_bar2").await.expect("ran");
    let vertical = crate::BarKind::VerticalButtonBar.setting_key();
    assert!(crate::settings::stored_bool(&db, vertical).await);
}

/// Over MCP a start as administrator, a question, parameters of the caller, a row stricter
/// than `Exec` and a window row are refused - whatever the gate judged before.
#[tokio::test]
async fn mcp_runs_only_what_its_class_allows() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    let mut admin = program("admin-test", "");
    admin.elevated = true;
    save(&engine, "em_admin", admin).await;
    save(&engine, "em_ask", program("ask-test", "?%N")).await;
    save(&engine, "em_window", rows("cm_AddPathToCmdline")).await;
    save(&engine, "em_plain", program("plain-mcp-test", "%N")).await;
    for (name, code) in [
        ("em_admin", "gui_only"),
        ("em_ask", "gui_only"),
        ("em_window", "user_command_window_only"),
    ] {
        let result = crate::read_guard::as_mcp(engine.run_user_command(name, None, None)).await;
        assert_eq!(result.expect_err("refused").code(), code, "{name}");
    }
    let with_params =
        crate::read_guard::as_mcp(engine.run_user_command("em_plain", None, Some("x".into())))
            .await;
    assert_eq!(with_params.expect_err("refused").code(), "gui_only");
    assert!(started("admin-test").is_empty());
    ran(crate::read_guard::as_mcp(engine.run_user_command("em_plain", None, None)).await);
    assert_eq!(started("plain-mcp-test").len(), 1);
}

#[tokio::test]
async fn a_start_menu_entry_runs_its_command() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    let saved = |item: StartItemDraft, parent: Option<i64>| {
        let engine = engine.clone();
        async move {
            match engine
                .dispatch(Command::SaveStartItem {
                    id: None,
                    parent,
                    item,
                })
                .await
                .expect("saved")
            {
                Outcome::UserCommands(outcome) => match *outcome {
                    UserCommandsOutcome::Saved { id } => id,
                    other => panic!("saved, got {other:?}"),
                },
                other => panic!("a user-commands outcome, got {other:?}"),
            }
        }
    };
    let tools = saved(
        StartItemDraft {
            kind: StartItemKind::Submenu,
            title: "Tools".into(),
            action: None,
        },
        None,
    )
    .await;
    let entry = saved(
        StartItemDraft {
            kind: StartItemKind::Item,
            title: "Editor".into(),
            action: Some(program("start-test", "%N")),
        },
        Some(tools),
    )
    .await;
    let result = engine
        .dispatch(Command::RunStartItem {
            id: entry,
            panel: None,
            params: None,
        })
        .await;
    assert_eq!(ran(result), (1, false));
    assert_eq!(started("start-test").len(), 1);
    let submenu = engine
        .dispatch(Command::RunStartItem {
            id: tools,
            panel: None,
            params: None,
        })
        .await;
    assert_eq!(submenu.expect_err("refused").code(), "start_menu_invalid");
}

/// `*` outside Windows: refused, and nothing starts - not even without elevation.
#[cfg(not(windows))]
#[tokio::test]
async fn an_elevated_command_outside_windows_starts_nothing() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    let mut admin = program("elevated-linux-test", "");
    admin.elevated = true;
    save(&engine, "em_admin", admin).await;
    assert_eq!(
        run(&engine, "em_admin").await.expect_err("refused").code(),
        "run_as_admin_unsupported"
    );
    assert!(started("elevated-linux-test").is_empty());
}

/// The names of a panel on a server reach no local program: `%M` and `%R` of an SFTP panel
/// answer `user_command_local_only`, as `%T` does, and nothing starts.
#[tokio::test]
async fn a_remote_panels_names_are_refused() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    {
        let mut state = engine.state();
        let right = state.panel_mut(PanelId::Right);
        right.path.backend = rc_vfs::BackendId::Sftp { connection_id: 1 };
        right.selection.insert("a.txt".to_owned());
    }
    for (name, params) in [("em_m", "%M"), ("em_r", "%R"), ("em_t", "%T")] {
        save(&engine, name, program("remote-test", params)).await;
        let error = run(&engine, name).await.expect_err("refused");
        assert_eq!(error.code(), "user_command_local_only", "{params}");
    }
    assert!(started("remote-test").is_empty());
}

/// A row runs as a task of its own, and still under the MCP call's pins: a panel moved since the
/// person was asked answers `panel_changed` from the row's command.
#[tokio::test]
async fn a_row_over_mcp_keeps_the_calls_panel_pins() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", ""), ("docs/b.txt", "")]).await;
    save(&engine, "em_refresh", rows("panel.refresh")).await;
    let pins = vec![crate::McpPin {
        panel: PanelId::Left,
        path: engine.panel(PanelId::Left).path,
    }];
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: "docs".to_owned(),
        })
        .await
        .expect("navigates");
    let refused = crate::with_mcp_pins(
        pins,
        crate::read_guard::as_mcp(engine.run_user_command("em_refresh", None, None)),
    )
    .await;
    assert_eq!(refused.expect_err("pinned").code(), "panel_changed");
    // Without pins the same row runs.
    ran(crate::read_guard::as_mcp(engine.run_user_command("em_refresh", None, None)).await);
}
