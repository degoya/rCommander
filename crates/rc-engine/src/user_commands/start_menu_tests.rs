//! The Start menu through the commands: rows saved, listed in menu order, moved and deleted.

use crate::{
    Command, CommandRef, Engine, EngineError, EngineEvent, Outcome, StartItemDraft, StartItemKind,
    UserCommandsOutcome, test_support,
};

fn outcome(result: Result<Outcome, EngineError>) -> UserCommandsOutcome {
    match result.expect("answered") {
        Outcome::UserCommands(outcome) => *outcome,
        other => panic!("a user-commands outcome, got {other:?}"),
    }
}

fn entry(title: &str) -> StartItemDraft {
    StartItemDraft {
        kind: StartItemKind::Item,
        title: title.into(),
        action: Some(CommandRef {
            command: "regedit.exe".into(),
            ..CommandRef::default()
        }),
    }
}

fn submenu(title: &str) -> StartItemDraft {
    StartItemDraft {
        kind: StartItemKind::Submenu,
        title: title.into(),
        action: None,
    }
}

async fn add(engine: &Engine, parent: Option<i64>, item: StartItemDraft) -> i64 {
    match outcome(
        engine
            .dispatch(Command::SaveStartItem {
                id: None,
                parent,
                item,
            })
            .await,
    ) {
        UserCommandsOutcome::Saved { id } => id,
        other => panic!("saved, got {other:?}"),
    }
}

async fn menu(engine: &Engine) -> Vec<(usize, String)> {
    engine
        .start_menu()
        .await
        .expect("listed")
        .into_iter()
        .map(|item| (item.depth, item.title))
        .collect()
}

#[tokio::test]
async fn the_menu_comes_in_menu_order_with_depths() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let tools = add(&engine, None, submenu("Tools")).await;
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::StartMenuChanged));
    add(&engine, Some(tools), entry("Registry")).await;
    let inner = add(&engine, Some(tools), submenu("More")).await;
    add(&engine, Some(inner), entry("Deep")).await;
    add(
        &engine,
        None,
        StartItemDraft {
            kind: StartItemKind::Separator,
            title: "ignored".into(),
            action: Some(CommandRef::default()),
        },
    )
    .await;
    add(&engine, None, entry("Notes")).await;
    assert_eq!(
        menu(&engine).await,
        [
            (0, "Tools".to_owned()),
            (1, "Registry".to_owned()),
            (1, "More".to_owned()),
            (2, "Deep".to_owned()),
            (0, String::new()),
            (0, "Notes".to_owned()),
        ]
    );
    let items = engine.start_menu().await.expect("listed");
    assert!(items[4].action.is_none(), "a separator has no command");

    // Notes to the top, Tools with everything in it deleted.
    outcome(
        engine
            .dispatch(Command::MoveStartItem {
                id: items[5].id,
                parent: None,
                position: Some(0),
            })
            .await,
    );
    let removed = outcome(
        engine
            .dispatch(Command::DeleteStartItem { id: tools })
            .await,
    );
    assert_eq!(removed, UserCommandsOutcome::Deleted { removed: 4 });
    assert_eq!(
        menu(&engine).await,
        [(0, "Notes".to_owned()), (0, String::new())]
    );
}

#[tokio::test]
async fn rows_that_do_not_fit_are_refused() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let refused = |item: StartItemDraft, parent: Option<i64>| {
        let engine = engine.clone();
        async move {
            engine
                .dispatch(Command::SaveStartItem {
                    id: None,
                    parent,
                    item,
                })
                .await
                .expect_err("refused")
                .code()
        }
    };
    assert_eq!(refused(entry(" "), None).await, "start_menu_invalid");
    assert_eq!(refused(submenu(""), None).await, "start_menu_invalid");
    let no_command = StartItemDraft {
        action: None,
        ..entry("Nothing")
    };
    assert_eq!(refused(no_command, None).await, "start_menu_invalid");
    let notes = add(&engine, None, entry("Notes")).await;
    assert_eq!(
        refused(entry("Inside"), Some(notes)).await,
        "start_menu_invalid",
        "only a submenu holds rows"
    );
    assert_eq!(
        refused(entry("Lost"), Some(999)).await,
        "start_menu_unknown"
    );
    let outer = add(&engine, None, submenu("Outer")).await;
    let inner = add(&engine, Some(outer), submenu("Inner")).await;
    let cycle = engine
        .dispatch(Command::MoveStartItem {
            id: outer,
            parent: Some(inner),
            position: None,
        })
        .await;
    assert_eq!(cycle.expect_err("refused").code(), "start_menu_invalid");
}
