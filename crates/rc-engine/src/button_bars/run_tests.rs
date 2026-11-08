//! Buttons run through the commands, their programs recorded in `launch::STARTED` instead of
//! started; each test looks for its own program's name.

use tokio::sync::broadcast::error::TryRecvError;

use crate::{
    AskSource, BarItem, BarSpec, ButtonAction, ButtonBarsOutcome, Command, CommandClass,
    CommandKind, CommandRef, Engine, EngineError, EngineEvent, Outcome, PanelId,
    button_bars::BarButton,
    recover::LockOrRecover as _,
    test_support,
    user_commands::launch::{Launch, STARTED},
};

fn button(action: CommandRef) -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::Command(action),
        icon: None,
        tooltip: String::new(),
    })
}

fn program(command: &str, params: &str) -> CommandRef {
    CommandRef {
        command: command.into(),
        params: params.into(),
        ..CommandRef::default()
    }
}

/// A new bar of `items`; its id.
async fn bar(engine: &Engine, name: &str, items: Vec<BarItem>) -> i64 {
    let saved = engine
        .dispatch(Command::SaveButtonBar {
            id: None,
            name: name.into(),
            spec: BarSpec {
                items,
                ..BarSpec::default()
            },
        })
        .await
        .expect("saved");
    match saved {
        Outcome::ButtonBars(outcome) => match *outcome {
            ButtonBarsOutcome::Saved { id } => id,
            other => panic!("saved, got {other:?}"),
        },
        other => panic!("a button-bars outcome, got {other:?}"),
    }
}

async fn run(
    engine: &Engine,
    bar: i64,
    button: usize,
    files: Vec<String>,
) -> Result<Outcome, EngineError> {
    engine
        .dispatch(Command::RunButton {
            bar,
            button,
            panel: Some(PanelId::Left),
            params: None,
            files,
        })
        .await
}

fn ran(result: Result<Outcome, EngineError>) -> (usize, bool) {
    match result.expect("ran") {
        Outcome::ButtonBars(outcome) => match *outcome {
            ButtonBarsOutcome::Ran { started, asking } => (started, asking),
            other => panic!("a run, got {other:?}"),
        },
        other => panic!("a button-bars outcome, got {other:?}"),
    }
}

fn started(program: &str) -> Vec<Launch> {
    STARTED
        .lock_or_recover("test")
        .iter()
        .filter(|launch| launch.program == program)
        .cloned()
        .collect()
}

/// An absolute path for a dropped file on this system.
fn dropped(name: &str) -> String {
    std::env::temp_dir()
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[tokio::test]
async fn a_button_starts_its_program_and_dropped_files_follow_its_parameters() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    let id = bar(
        &engine,
        "Tools",
        vec![BarItem::Separator, button(program("bar-np-test", "-n"))],
    )
    .await;
    let files = vec![dropped("one file.txt"), dropped("two.txt")];
    assert_eq!(ran(run(&engine, id, 1, files.clone()).await), (1, false));
    let launch = started("bar-np-test").pop().expect("started");
    if cfg!(windows) {
        assert_eq!(
            launch.args,
            [format!("-n \"{}\" \"{}\"", files[0], files[1])]
        );
    } else {
        assert_eq!(
            launch.args,
            ["-n".to_owned(), files[0].clone(), files[1].clone()]
        );
    }
    assert!(launch.values.contains(&files[0]), "checked as a value");
}

#[tokio::test]
async fn a_separator_a_sub_bar_and_a_place_past_the_end_run_nothing() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    let inner = bar(
        &engine,
        "Inner",
        vec![button(program("bar-inner-test", ""))],
    )
    .await;
    let outer = bar(
        &engine,
        "Outer",
        vec![
            BarItem::Separator,
            BarItem::Button(BarButton {
                action: ButtonAction::SubBar(inner),
                icon: None,
                tooltip: "More".into(),
            }),
        ],
    )
    .await;
    let code = |result: Result<Outcome, EngineError>| result.expect_err("refused").code();
    assert_eq!(
        code(run(&engine, outer, 0, Vec::new()).await),
        "button_bar_no_button"
    );
    assert_eq!(
        code(run(&engine, outer, 1, Vec::new()).await),
        "button_bar_sub_bar"
    );
    assert_eq!(
        code(run(&engine, outer, 9, Vec::new()).await),
        "button_bar_no_button"
    );
    assert_eq!(
        code(run(&engine, 999, 0, Vec::new()).await),
        "button_bar_unknown"
    );
    assert!(started("bar-inner-test").is_empty());
}

#[tokio::test]
async fn internal_rows_take_no_files_and_files_must_be_absolute() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    let id = bar(
        &engine,
        "Rows",
        vec![
            button(CommandRef {
                kind: CommandKind::Internal,
                command: "panel.refresh".into(),
                ..CommandRef::default()
            }),
            button(program("bar-rel-test", "")),
        ],
    )
    .await;
    let error = run(&engine, id, 0, vec![dropped("a.txt")])
        .await
        .expect_err("no files");
    assert_eq!(error.code(), "button_bar_no_files");
    for file in ["relative.txt", "", "a\nb"] {
        let error = run(&engine, id, 1, vec![file.to_owned()])
            .await
            .expect_err("no local path");
        assert_eq!(error.code(), "button_bar_file_invalid", "{file:?}");
    }
    assert!(started("bar-rel-test").is_empty());
    assert_eq!(ran(run(&engine, id, 0, Vec::new()).await), (0, false));
}

/// `?` hands the parameters to the window with the button and its files as the source.
#[tokio::test]
async fn a_question_mark_asks_with_the_button_and_its_files() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    let id = bar(&engine, "Ask", vec![button(program("bar-ask-test", "?-x"))]).await;
    let mut events = engine.subscribe();
    let files = vec![dropped("b.txt")];
    assert_eq!(ran(run(&engine, id, 0, files.clone()).await), (0, true));
    let request = loop {
        match events.try_recv() {
            Ok(EngineEvent::UserCommandAsk(request)) => break request,
            Ok(_) | Err(TryRecvError::Lagged(_)) => {}
            Err(TryRecvError::Empty | TryRecvError::Closed) => panic!("no question"),
        }
    };
    assert_eq!(
        request.source,
        AskSource::Button {
            bar: id,
            button: 0,
            files
        }
    );
    assert!(started("bar-ask-test").is_empty());
}

#[tokio::test]
async fn over_mcp_files_parameters_and_an_elevated_command_are_the_persons() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "")]).await;
    let mut elevated = program("bar-admin-test", "");
    elevated.elevated = true;
    let id = bar(
        &engine,
        "Mcp",
        vec![button(program("bar-plain-test", "")), button(elevated)],
    )
    .await;
    let call = |button: usize, files: Vec<String>, params: Option<String>| Command::RunButton {
        bar: id,
        button,
        panel: None,
        params,
        files,
    };
    assert_eq!(
        engine.effective_class(&call(0, Vec::new(), None)).await,
        CommandClass::Exec
    );
    assert_eq!(
        engine
            .effective_class(&call(0, vec![dropped("a.txt")], None))
            .await,
        CommandClass::GuiOnly
    );
    assert_eq!(
        engine
            .effective_class(&call(0, Vec::new(), Some("-x".into())))
            .await,
        CommandClass::GuiOnly
    );
    assert_eq!(
        engine.effective_class(&call(1, Vec::new(), None)).await,
        CommandClass::GuiOnly
    );
}
