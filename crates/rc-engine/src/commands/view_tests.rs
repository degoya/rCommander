use std::fs;

use tempfile::TempDir;

use crate::test_support;
use crate::test_support::run_panel;
use crate::{
    Command, Engine, EngineEvent, ListMode, Outcome, PanelChange, PanelId, SortKey, SortOrder,
};

fn names(engine: &Engine, panel: PanelId) -> Vec<String> {
    engine
        .panel(panel)
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

fn sort(key: SortKey, descending: Option<bool>) -> Command {
    Command::Sort {
        panel: None,
        key,
        descending,
    }
}

/// `sub/` with `b2.txt`, `b10.txt` and `a.txt`; `small.txt` (1 byte), `mid.md` (20) and
/// `big.bin` (300); `.hidden`, hidden on every platform.
fn tree() -> TempDir {
    let (mid, big) = ("\0".repeat(20), "\0".repeat(300));
    let root = test_support::tree(&[
        ("sub/b2.txt", "x"),
        ("sub/b10.txt", "x"),
        ("sub/a.txt", "x"),
        ("small.txt", "\0"),
        ("mid.md", mid.as_str()),
        ("big.bin", big.as_str()),
    ]);
    test_support::hidden_file(&root.path().join(".hidden"), "x");
    root
}

#[tokio::test]
async fn sorting_keeps_the_cursor_on_its_entry_and_turns_on_the_same_key() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    run_panel(
        &engine,
        Command::SetCursor {
            panel: None,
            target: crate::CursorTarget::Name("mid.md".into()),
        },
    )
    .await;
    let mut events = engine.subscribe();

    let largest_first = run_panel(&engine, sort(SortKey::Size, None)).await;
    assert_eq!(
        largest_first.sort,
        SortOrder {
            key: SortKey::Size,
            descending: true
        }
    );
    assert_eq!(
        names(&engine, PanelId::Left),
        ["sub", "big.bin", "mid.md", "small.txt"]
    );
    assert_eq!(
        largest_first.cursor.map(|cursor| cursor.name).as_deref(),
        Some("mid.md")
    );
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::Panel {
            panel: PanelId::Left,
            change: PanelChange::Listing
        }
    );

    run_panel(&engine, sort(SortKey::Size, None)).await;
    assert_eq!(
        names(&engine, PanelId::Left),
        ["sub", "small.txt", "mid.md", "big.bin"]
    );
    run_panel(&engine, sort(SortKey::Name, Some(true))).await;
    assert_eq!(
        names(&engine, PanelId::Left),
        ["sub", "small.txt", "mid.md", "big.bin"]
    );
    assert_eq!(
        names(&engine, PanelId::Right),
        ["sub", "big.bin", "mid.md", "small.txt"],
        "the other panel keeps its own order"
    );
}

#[tokio::test]
async fn order_and_view_stay_when_the_panel_moves() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    run_panel(&engine, sort(SortKey::Name, Some(true))).await;
    let brief = run_panel(
        &engine,
        Command::SetListMode {
            panel: None,
            mode: ListMode::Brief,
        },
    )
    .await;
    assert_eq!(brief.mode, ListMode::Brief);
    let inside = run_panel(
        &engine,
        Command::Navigate {
            panel: None,
            path: "sub".into(),
        },
    )
    .await;
    assert_eq!(
        (inside.mode, inside.sort.descending),
        (ListMode::Brief, true)
    );
    assert_eq!(
        names(&engine, PanelId::Left),
        ["b10.txt", "b2.txt", "a.txt"],
        "natural order, descending"
    );
    assert_eq!(engine.panel(PanelId::Right).mode, ListMode::Full);
}

#[tokio::test]
async fn hidden_entries_show_in_both_panels_only_when_switched_on() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    assert!(!names(&engine, PanelId::Left).contains(&".hidden".to_owned()));
    let Outcome::Listing { entries, .. } = engine
        .dispatch(Command::ListDir {
            panel: None,
            path: None,
        })
        .await
        .expect("lists")
    else {
        panic!("a listing");
    };
    assert!(
        entries.iter().any(|entry| entry.name == ".hidden"),
        "list_dir shows them, marked"
    );

    let shown = run_panel(&engine, Command::ToggleHidden { show: None }).await;
    assert!(shown.show_hidden);
    for panel in PanelId::BOTH {
        assert!(names(&engine, panel).contains(&".hidden".to_owned()));
    }
    let hidden = run_panel(&engine, Command::ToggleHidden { show: Some(false) }).await;
    assert!(!hidden.show_hidden);
    assert!(!names(&engine, PanelId::Right).contains(&".hidden".to_owned()));
}

#[tokio::test]
async fn the_hidden_switch_is_stored_and_restored() {
    let root = tree();
    let db = rc_db::Db::open_in_memory().await.expect("db");
    let first = test_support::headless(root.path()).await;
    first.attach_db(db.clone());
    run_panel(&first, Command::ToggleHidden { show: Some(true) }).await;

    let next_run = test_support::headless(root.path()).await;
    next_run.attach_db(db);
    assert!(!next_run.panel(PanelId::Left).show_hidden);
    next_run.restore_settings().await;
    assert!(next_run.panel(PanelId::Left).show_hidden);
    assert!(names(&next_run, PanelId::Right).contains(&".hidden".to_owned()));
}

/// `sort.names` switched over `settings.set` sorts both panels again at once (owner, 2026-10-19);
/// back to Windows' order, `Beta` follows `alpha` again.
#[tokio::test]
async fn a_changed_name_order_sorts_the_panels_again() {
    let root = tempfile::tempdir().expect("temp dir");
    for name in ["alpha", "Beta"] {
        fs::write(root.path().join(name), b"x").expect("write");
    }
    let engine = test_support::headless(root.path()).await;
    assert!(engine.attach_db(rc_db::Db::open_in_memory().await.expect("db")));
    assert_eq!(names(&engine, PanelId::Left), ["alpha", "Beta"]);
    let set = |value: &str| Command::SetSetting {
        key: crate::settings::SORT_NAMES_KEY.to_owned(),
        value: serde_json::json!(value),
    };
    engine.dispatch(set("linux")).await.expect("stored");
    assert_eq!(names(&engine, PanelId::Left), ["Beta", "alpha"]);
    engine.dispatch(set("windows")).await.expect("stored");
    assert_eq!(names(&engine, PanelId::Left), ["alpha", "Beta"]);
}
