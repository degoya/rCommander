//! The bars through the commands: saved, listed, renamed and deleted, sub-bars checked across
//! every bar, a `.bar` file imported with its sub-bars, each change announced; and the bars of
//! the window switched by `view.toggle_bar`.

use super::{free_name, post_order};
use crate::{
    BarItem, BarKind, BarSpec, ButtonAction, ButtonBarsOutcome, Command, CommandRef, Engine,
    EngineError, EngineEvent, Outcome,
    button_bars::{
        BarButton, DEFAULT_BAR_NAME, ImportPreview, PreviewBar, PreviewItem, PreviewResult,
    },
    settings::stored_bool,
    test_support,
};

fn outcome(result: Result<Outcome, EngineError>) -> ButtonBarsOutcome {
    match result.expect("answered") {
        Outcome::ButtonBars(outcome) => *outcome,
        other => panic!("a button-bars outcome, got {other:?}"),
    }
}

fn sub_bar(id: i64) -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::SubBar(id),
        icon: None,
        tooltip: String::new(),
    })
}

fn program(command: &str) -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::Command(CommandRef {
            command: command.into(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip: String::new(),
    })
}

fn spec(items: Vec<BarItem>) -> BarSpec {
    BarSpec {
        items,
        ..BarSpec::default()
    }
}

async fn save(
    engine: &Engine,
    id: Option<i64>,
    name: &str,
    items: Vec<BarItem>,
) -> Result<i64, String> {
    match engine
        .dispatch(Command::SaveButtonBar {
            id,
            name: name.into(),
            spec: spec(items),
        })
        .await
    {
        Ok(Outcome::ButtonBars(outcome)) => match *outcome {
            ButtonBarsOutcome::Saved { id } => Ok(id),
            other => panic!("saved, got {other:?}"),
        },
        Ok(other) => panic!("a button-bars outcome, got {other:?}"),
        Err(error) => Err(error.code().to_owned()),
    }
}

#[tokio::test]
async fn a_bar_is_saved_listed_renamed_and_deleted() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let id = save(&engine, None, " Tools ", vec![program("np.exe")])
        .await
        .expect("saved");
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::ButtonBarsChanged));
    let ButtonBarsOutcome::Bars { bars } =
        outcome(engine.dispatch(Command::ListButtonBars {}).await)
    else {
        panic!("a list");
    };
    assert_eq!(bars.len(), 2, "the first start's bar and the new one");
    assert_eq!(bars[0].name, DEFAULT_BAR_NAME);
    assert_eq!(bars[1].name, "Tools");

    outcome(
        engine
            .dispatch(Command::RenameButtonBar {
                id,
                name: "More".into(),
            })
            .await,
    );
    let ButtonBarsOutcome::Bar { bar } =
        outcome(engine.dispatch(Command::GetButtonBar { id }).await)
    else {
        panic!("a bar");
    };
    assert_eq!(bar.name, "More");
    assert_eq!(bar.spec.items.len(), 1);

    assert_eq!(
        save(&engine, None, "default", Vec::new()).await,
        Err("button_bar_name_taken".to_owned())
    );
    assert_eq!(
        save(&engine, None, "  ", Vec::new()).await,
        Err("button_bar_name".to_owned())
    );

    outcome(engine.dispatch(Command::DeleteButtonBar { id }).await);
    let error = engine
        .dispatch(Command::GetButtonBar { id })
        .await
        .expect_err("gone");
    assert_eq!(error.code(), "button_bar_unknown");
}

#[tokio::test]
async fn sub_bars_are_checked_across_every_bar() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let leaf = save(&engine, None, "Leaf", Vec::new())
        .await
        .expect("saved");
    let top = save(&engine, None, "Top", vec![sub_bar(leaf)])
        .await
        .expect("a sub-bar that exists");
    assert_eq!(
        save(&engine, Some(leaf), "Leaf", vec![sub_bar(top)]).await,
        Err("button_bar_cycle".to_owned()),
        "the leaf would open the bar above it"
    );
    assert_eq!(
        save(&engine, None, "Lost", vec![sub_bar(999)]).await,
        Err("button_bar_unknown".to_owned())
    );
    let error = engine
        .dispatch(Command::DeleteButtonBar { id: leaf })
        .await
        .expect_err("in use");
    assert_eq!(error.code(), "button_bar_in_use");

    // Four levels below "Three" are allowed, a fifth is not.
    let one = save(&engine, None, "One", vec![sub_bar(top)])
        .await
        .expect("1");
    let two = save(&engine, None, "Two", vec![sub_bar(one)])
        .await
        .expect("2");
    let three = save(&engine, None, "Three", vec![sub_bar(two)])
        .await
        .expect("3: Three > Two > One > Top > Leaf is four deep");
    assert_eq!(
        save(&engine, None, "Four", vec![sub_bar(three)]).await,
        Err("button_bar_too_deep".to_owned())
    );
}

#[tokio::test]
async fn toggle_bar_turns_its_setting_over() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let db = engine.db().expect("a database").clone();
    assert!(stored_bool(&db, BarKind::StatusBar.setting_key()).await);
    engine
        .dispatch(Command::ToggleBar {
            bar: BarKind::StatusBar,
        })
        .await
        .expect("switched");
    assert!(!stored_bool(&db, BarKind::StatusBar.setting_key()).await);
    assert!(!stored_bool(&db, BarKind::VerticalButtonBar.setting_key()).await);
    engine
        .dispatch(Command::ToggleBar {
            bar: BarKind::VerticalButtonBar,
        })
        .await
        .expect("switched");
    assert!(stored_bool(&db, BarKind::VerticalButtonBar.setting_key()).await);
}

#[tokio::test]
async fn a_bar_file_is_imported_with_its_sub_bars() {
    let (root, engine) = test_support::engine_with_db(&[]).await;
    let main = root.path().join("default.bar");
    std::fs::write(
        &main,
        "[Buttonbar]\r\nButtoncount=3\r\ncmd1=cm_SrcFull\r\ncmd2=\r\ncmd3=tools.bar\r\nmenu3=Tools\r\n",
    )
    .expect("written");
    std::fs::write(
        root.path().join("tools.bar"),
        "[Buttonbar]\r\nButtoncount=1\r\ncmd1=notepad.exe\r\n",
    )
    .expect("written");
    let path = main.to_string_lossy().into_owned();
    let ButtonBarsOutcome::Preview { preview } = outcome(
        engine
            .dispatch(Command::PreviewButtonBarImport { path: path.clone() })
            .await,
    ) else {
        panic!("a preview");
    };
    assert_eq!(preview.bars.len(), 2);
    // A bar of that name is there already.
    save(&engine, None, "Tools", Vec::new())
        .await
        .expect("saved");
    let ButtonBarsOutcome::Imported { bars } =
        outcome(engine.dispatch(Command::ImportButtonBar { path }).await)
    else {
        panic!("imported");
    };
    assert_eq!(bars.len(), 2);
    let ButtonBarsOutcome::Bar { bar: top } =
        outcome(engine.dispatch(Command::GetButtonBar { id: bars[0] }).await)
    else {
        panic!("a bar");
    };
    assert_eq!(top.name, "default (2)", "\"Default\" is the first start's");
    assert_eq!(top.spec.items.len(), 3);
    assert_eq!(top.spec.sub_bars(), [bars[1]]);
    let ButtonBarsOutcome::Bar { bar: tools } =
        outcome(engine.dispatch(Command::GetButtonBar { id: bars[1] }).await)
    else {
        panic!("a bar");
    };
    assert_eq!(tools.name, "tools (2)");

    let error = engine
        .dispatch(Command::ImportButtonBar {
            path: "relative.bar".into(),
        })
        .await
        .expect_err("no absolute path");
    assert_eq!(error.code(), "button_bar_import");
}

#[test]
fn a_taken_name_gets_a_number() {
    let taken = vec!["tools".to_owned(), "tools (2)".to_owned()];
    assert_eq!(free_name("Other", &taken), "Other");
    assert_eq!(free_name("Tools", &taken), "Tools (3)");
    let long = "x".repeat(rc_db::MAX_BUTTON_BAR_NAME);
    let named = free_name(&long, std::slice::from_ref(&long));
    assert_eq!(named.chars().count(), rc_db::MAX_BUTTON_BAR_NAME);
    assert!(named.ends_with(" (2)"));
}

#[test]
fn sub_bars_are_made_before_the_bars_that_open_them() {
    let item = |result| PreviewItem {
        index: 1,
        tooltip: String::new(),
        icon: None,
        result,
    };
    let bar = |items| PreviewBar {
        file: String::new(),
        name: String::new(),
        items,
        dropped: 0,
    };
    let preview = ImportPreview {
        bars: vec![
            bar(vec![item(PreviewResult::SubBar { bar: 1 })]),
            bar(vec![item(PreviewResult::SubBar { bar: 2 })]),
            bar(vec![item(PreviewResult::Separator)]),
        ],
    };
    assert_eq!(post_order(&preview), [2, 1, 0]);
}

/// A button's symbol of the collection is saved by its name; a name the collection does not have
/// is refused (RC-110-10).
#[tokio::test]
async fn a_symbol_of_the_collection_is_checked_on_saving() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let with_symbol = |path: &str| {
        let mut item = program("np.exe");
        if let BarItem::Button(button) = &mut item {
            button.icon = Some(crate::button_bars::IconRef {
                path: path.into(),
                index: 0,
            });
        }
        item
    };
    let id = save(
        &engine,
        None,
        "Symbols",
        vec![with_symbol("rc-symbol:folder")],
    )
    .await
    .expect("saved");
    let ButtonBarsOutcome::Bar { bar } =
        outcome(engine.dispatch(Command::GetButtonBar { id }).await)
    else {
        panic!("a bar");
    };
    let icon = bar.spec.items[0]
        .button()
        .and_then(|button| button.icon.clone());
    assert_eq!(
        icon.map(|icon| icon.path),
        Some("rc-symbol:folder".to_owned())
    );
    assert_eq!(
        save(
            &engine,
            None,
            "Gone",
            vec![with_symbol("rc-symbol:no_such_symbol")]
        )
        .await,
        Err("button_bar_invalid".to_owned())
    );
}
