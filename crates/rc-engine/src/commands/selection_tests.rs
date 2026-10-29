use std::fs;

use crate::test_support;
use crate::test_support::run_panel;
use crate::{Command, Engine, EngineEvent, PanelChange, PanelId, PanelSnapshot};

#[tokio::test]
async fn all_none_and_inverted_cover_files_and_directories() {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir(root.path().join("dir")).expect("mkdir");
    fs::write(root.path().join("a.txt"), b"a").expect("write");
    fs::write(root.path().join("b.txt"), b"b").expect("write");
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();

    let all = run_panel(&engine, Command::SelectAll { panel: None }).await;
    assert_eq!(all.selection, ["a.txt", "b.txt", "dir"]);
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::Panel {
            panel: PanelId::Left,
            change: PanelChange::Selection
        }
    );
    let none = run_panel(&engine, Command::DeselectAll { panel: None }).await;
    assert!(none.selection.is_empty());

    run_panel(
        &engine,
        Command::ToggleSelection {
            panel: None,
            names: vec!["a.txt".into()],
        },
    )
    .await;
    let inverted = run_panel(&engine, Command::InvertSelection { panel: None }).await;
    assert_eq!(inverted.selection, ["b.txt", "dir"]);
}

#[tokio::test]
async fn refresh_reads_the_directory_again_and_keeps_cursor_and_selection() {
    let root = tempfile::tempdir().expect("temp dir");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.path().join(name), b"x").expect("write");
    }
    let engine = test_support::headless(root.path()).await;
    run_panel(
        &engine,
        Command::ToggleSelection {
            panel: None,
            names: vec!["a.txt".into(), "c.txt".into()],
        },
    )
    .await;
    run_panel(
        &engine,
        Command::SetCursor {
            panel: None,
            target: crate::CursorTarget::Name("b.txt".into()),
        },
    )
    .await;
    fs::write(root.path().join("0new.txt"), b"x").expect("write");
    fs::remove_file(root.path().join("c.txt")).expect("rm");

    let reread = run_panel(&engine, Command::Refresh { panel: None }).await;
    assert_eq!(reread.entry_count, 3);
    assert_eq!(
        reread.cursor.map(|cursor| cursor.name).as_deref(),
        Some("b.txt")
    );
    assert_eq!(reread.selection, ["a.txt"]);
}

/// `a.txt`, `b.TXT`, `c.md`, `d` and the directory `docs/`; in panel order `docs`, `a.txt`,
/// `b.TXT`, `c.md`, `d`.
async fn mixed() -> (tempfile::TempDir, Engine) {
    test_support::engine(&[
        ("docs/", ""),
        ("a.txt", "x"),
        ("b.TXT", "x"),
        ("c.md", "x"),
        ("d", "x"),
    ])
    .await
}

fn pattern(mask: Option<&str>, select: bool) -> Command {
    let mask = mask.map(str::to_owned);
    if select {
        Command::SelectByMask(crate::MaskRequest { panel: None, mask })
    } else {
        Command::DeselectByMask(crate::MaskRequest { panel: None, mask })
    }
}

fn cursor_on(name: &str) -> Command {
    Command::SetCursor {
        panel: None,
        target: crate::CursorTarget::Name(name.into()),
    }
}

#[tokio::test]
async fn a_mask_selects_files_and_deselects_what_it_matches() {
    let (_root, engine) = mixed().await;
    let txt = run_panel(&engine, pattern(Some("*.txt"), true)).await;
    assert_eq!(txt.selection, ["a.txt", "b.TXT"], "case is ignored");
    let all = run_panel(&engine, pattern(None, true)).await;
    assert_eq!(all.selection, ["a.txt", "b.TXT", "c.md", "d"], "files only");
    let kept = run_panel(&engine, pattern(Some("*.*|*.md"), false)).await;
    assert_eq!(kept.selection, ["c.md"]);
    let dirs = run_panel(&engine, pattern(Some("*\\"), true)).await;
    assert_eq!(dirs.selection, ["c.md", "docs"]);
}

#[tokio::test]
async fn the_cursor_files_extension_selects_its_kind() {
    let (_root, engine) = mixed().await;
    let same = |select| Command::SelectExtension {
        panel: None,
        select,
    };
    run_panel(&engine, cursor_on("a.txt")).await;
    assert_eq!(
        run_panel(&engine, same(true)).await.selection,
        ["a.txt", "b.TXT"]
    );
    run_panel(&engine, cursor_on("d")).await;
    assert_eq!(
        run_panel(&engine, same(true)).await.selection,
        ["a.txt", "b.TXT", "d"],
        "no extension is an extension too"
    );
    run_panel(&engine, cursor_on("docs")).await;
    assert_eq!(run_panel(&engine, same(true)).await.selection.len(), 3);
    run_panel(&engine, cursor_on("b.TXT")).await;
    assert_eq!(run_panel(&engine, same(false)).await.selection, ["d"]);
}

#[tokio::test]
async fn num_slash_brings_back_the_selection_a_file_operation_took() {
    let (_root, engine) = mixed().await;
    run_panel(&engine, pattern(Some("*.txt"), true)).await;
    engine
        .dispatch(Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources: Vec::new(),
            target: Some("docs".into()),
            options: rc_ops::JobOptions::default(),
        }))
        .await
        .expect("a copy job");
    run_panel(&engine, Command::DeselectAll { panel: None }).await;
    let back = run_panel(&engine, Command::RestoreSelection { panel: None }).await;
    assert_eq!(back.selection, ["a.txt", "b.TXT"]);
}

#[tokio::test]
async fn shift_and_a_cursor_key_select_a_range() {
    use crate::CursorStep;
    let (_root, engine) = mixed().await;
    let range = |step| Command::SelectRange { panel: None, step };
    let cursor = |snapshot: &PanelSnapshot| snapshot.cursor.as_ref().map(|cursor| cursor.index);

    run_panel(&engine, range(CursorStep::Down)).await;
    let two = run_panel(&engine, range(CursorStep::Down)).await;
    assert_eq!(
        (
            two.selection.iter().cloned().collect::<Vec<_>>(),
            cursor(&two)
        ),
        (vec!["a.txt".to_owned(), "docs".to_owned()], Some(2))
    );
    let to_end = run_panel(&engine, range(CursorStep::Last)).await;
    assert_eq!(
        to_end.selection.len(),
        5,
        "Shift+End takes the last entry too"
    );
    assert_eq!(cursor(&to_end), Some(4));
    let up = run_panel(&engine, range(CursorStep::Up)).await;
    assert_eq!(cursor(&up), Some(3));
    assert!(
        !up.selection.contains(&"d".to_owned()),
        "a selected start deselects"
    );
    let to_top = run_panel(&engine, range(CursorStep::First)).await;
    assert!(to_top.selection.is_empty());
    assert_eq!(cursor(&to_top), Some(0));
    let edge = run_panel(&engine, range(CursorStep::Up)).await;
    assert_eq!(
        edge.selection,
        ["docs"],
        "a step cut short takes the entry it stays on"
    );
}

/// `selection.include_folders` set the way MCP's `settings_set` sets it.
async fn include_folders(engine: &Engine, on: bool) {
    engine
        .dispatch_mcp(Command::SetSetting {
            key: crate::settings::SELECT_FOLDERS_KEY.into(),
            value: serde_json::json!(on),
        })
        .await
        .expect("an MCP caller may set it");
}

/// RC-020-08: off, Num+ `*.*` takes files only; on, folders too (an engine listing has no `..`);
/// a folder mask works as before either way.
#[tokio::test]
async fn num_plus_takes_folders_only_when_the_setting_is_on() {
    let (root, engine) = mixed().await;
    fs::create_dir(root.path().join("src.txt")).expect("mkdir");
    run_panel(&engine, Command::Refresh { panel: None }).await;
    let db = rc_db::Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db));

    let files = run_panel(&engine, pattern(None, true)).await;
    assert_eq!(
        files.selection,
        ["a.txt", "b.TXT", "c.md", "d"],
        "off: files only"
    );
    run_panel(&engine, Command::DeselectAll { panel: None }).await;

    include_folders(&engine, true).await;
    let all = run_panel(&engine, pattern(Some("*.*"), true)).await;
    assert_eq!(
        all.selection,
        ["a.txt", "b.TXT", "c.md", "d", "docs", "src.txt"]
    );
    assert!(!all.selection.iter().any(|name| name == ".."));
    let kept = run_panel(&engine, pattern(Some("*.txt"), false)).await;
    assert_eq!(
        kept.selection,
        ["c.md", "d", "docs"],
        "Num- deselects folders too"
    );
    run_panel(&engine, Command::DeselectAll { panel: None }).await;
    let dirs = run_panel(&engine, pattern(Some("*\\"), true)).await;
    assert_eq!(
        dirs.selection,
        ["docs", "src.txt"],
        "a folder mask as before"
    );
    run_panel(&engine, Command::DeselectAll { panel: None }).await;
    let excluded = run_panel(&engine, pattern(Some("*.*|d*"), true)).await;
    assert_eq!(
        excluded.selection,
        ["a.txt", "b.TXT", "c.md", "src.txt"],
        "an exclusion leaves folders out too"
    );

    run_panel(&engine, Command::DeselectAll { panel: None }).await;
    include_folders(&engine, false).await;
    let again = run_panel(&engine, pattern(None, true)).await;
    assert_eq!(
        again.selection,
        ["a.txt", "b.TXT", "c.md", "d"],
        "takes effect at once"
    );
}

/// Alt+Num+ stays with files when folders take part in the selection.
#[tokio::test]
async fn the_extension_selection_ignores_the_folder_setting() {
    let (root, engine) = mixed().await;
    fs::create_dir(root.path().join("notes.txt")).expect("mkdir");
    run_panel(&engine, Command::Refresh { panel: None }).await;
    assert!(engine.attach_db(rc_db::Db::open_in_memory().await.expect("db")));
    include_folders(&engine, true).await;
    run_panel(&engine, cursor_on("a.txt")).await;
    let same = run_panel(
        &engine,
        Command::SelectExtension {
            panel: None,
            select: true,
        },
    )
    .await;
    assert_eq!(same.selection, ["a.txt", "b.TXT"]);
}
