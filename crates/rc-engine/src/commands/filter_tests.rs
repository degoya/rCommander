use std::fs;

use tempfile::TempDir;

use crate::test_support;
use crate::test_support::run_panel;
use crate::{Command, CursorTarget, Engine, PanelId, SortKey};

/// `sub/` (with `inner.txt`), then `a.txt`, `b.md`, `notes.txt`, `todo.md`.
fn tree() -> TempDir {
    test_support::tree(&[
        ("sub/inner.txt", "x"),
        ("a.txt", "x"),
        ("b.md", "x"),
        ("notes.txt", "x"),
        ("todo.md", "x"),
    ])
}

fn filter(pattern: Option<&str>) -> Command {
    Command::Filter {
        panel: None,
        pattern: pattern.map(str::to_owned),
        last: false,
    }
}

fn again() -> Command {
    Command::Filter {
        panel: None,
        pattern: None,
        last: true,
    }
}

fn names(engine: &Engine) -> Vec<String> {
    engine
        .panel(PanelId::Left)
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

#[tokio::test]
async fn a_filter_shows_only_what_matches_and_says_so() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let snapshot = run_panel(&engine, filter(Some("*.md"))).await;
    assert_eq!(names(&engine), ["b.md", "todo.md"]);
    assert_eq!(snapshot.filter.as_deref(), Some("*.md"));
    assert_eq!(snapshot.entry_count, 2);
    let json = serde_json::to_value(&snapshot).expect("serializes");
    assert_eq!(json["filter"], "*.md");

    run_panel(&engine, filter(Some("t?do*"))).await;
    assert_eq!(names(&engine), ["todo.md"]);
    run_panel(&engine, filter(Some("OTE"))).await;
    assert_eq!(
        names(&engine),
        ["notes.txt"],
        "a text without wildcards: anywhere"
    );
    assert_eq!(
        engine.panel(PanelId::Right).filter.pattern,
        None,
        "one panel only"
    );
}

#[tokio::test]
async fn no_pattern_or_a_blank_one_shows_everything_again() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    run_panel(&engine, filter(Some("*.md"))).await;
    let snapshot = run_panel(&engine, filter(None)).await;
    assert_eq!(
        names(&engine),
        ["sub", "a.txt", "b.md", "notes.txt", "todo.md"]
    );
    assert_eq!(snapshot.filter, None);
    let json = serde_json::to_value(&snapshot).expect("serializes");
    assert!(json.get("filter").is_none(), "{json}");
    run_panel(&engine, filter(Some("*.md"))).await;
    run_panel(&engine, filter(Some("   "))).await;
    assert_eq!(names(&engine).len(), 5);
}

#[tokio::test]
async fn the_cursor_stays_on_its_entry_and_hidden_entries_leave_the_selection() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: CursorTarget::Name("todo.md".into()),
        })
        .await
        .expect("cursor");
    engine
        .dispatch(Command::SelectAll { panel: None })
        .await
        .expect("select");
    let snapshot = run_panel(&engine, filter(Some("*.md"))).await;
    assert_eq!(snapshot.cursor.expect("a cursor").name, "todo.md");
    assert_eq!(snapshot.selection, ["b.md", "todo.md"]);

    // The cursor's entry hidden: the first entry shown.
    let snapshot = run_panel(&engine, filter(Some("*.txt"))).await;
    assert_eq!(snapshot.cursor.expect("a cursor").name, "a.txt");
    assert!(
        snapshot.selection.is_empty(),
        "nothing hidden stays selected"
    );
}

#[tokio::test]
async fn sort_and_refresh_keep_the_filter_another_folder_drops_it() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    run_panel(&engine, filter(Some("*.txt"))).await;
    run_panel(
        &engine,
        Command::Sort {
            panel: None,
            key: SortKey::Name,
            descending: Some(true),
        },
    )
    .await;
    assert_eq!(names(&engine), ["notes.txt", "a.txt"]);
    fs::write(root.path().join("new.txt"), b"x").expect("write");
    fs::write(root.path().join("new.md"), b"x").expect("write");
    run_panel(&engine, Command::Refresh { panel: None }).await;
    assert_eq!(names(&engine), ["notes.txt", "new.txt", "a.txt"]);

    let inside = run_panel(
        &engine,
        Command::Navigate {
            panel: None,
            path: "sub".into(),
        },
    )
    .await;
    assert_eq!(inside.filter, None);
    assert_eq!(names(&engine), ["inner.txt"]);
    let back = run_panel(
        &engine,
        Command::Navigate {
            panel: None,
            path: "..".into(),
        },
    )
    .await;
    assert_eq!(back.filter, None, "the filter went with the folder");
    assert_eq!(names(&engine).len(), 7);
}

#[tokio::test]
async fn ctrl_shift_s_applies_the_last_filter_again() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let nothing = run_panel(&engine, again()).await;
    assert_eq!(nothing.filter, None, "no filter yet: nothing changes");
    run_panel(&engine, filter(Some("*.md"))).await;
    run_panel(&engine, filter(None)).await;
    run_panel(
        &engine,
        Command::Navigate {
            panel: None,
            path: "sub".into(),
        },
    )
    .await;
    run_panel(
        &engine,
        Command::Navigate {
            panel: None,
            path: "..".into(),
        },
    )
    .await;
    let snapshot = run_panel(&engine, again()).await;
    assert_eq!(snapshot.filter.as_deref(), Some("*.md"));
    assert_eq!(names(&engine), ["b.md", "todo.md"]);
}

#[tokio::test]
async fn a_long_or_broken_filter_is_refused() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let long = "x".repeat(crate::quick_search::MAX_PATTERN + 1);
    for pattern in [long.as_str(), "a\nb"] {
        let error = engine
            .dispatch(filter(Some(pattern)))
            .await
            .expect_err("refused");
        assert_eq!(error.code(), "filter_invalid");
    }
    assert_eq!(names(&engine).len(), 5, "the panel is unchanged");
}
