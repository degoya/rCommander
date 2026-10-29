use std::{fs, path::MAIN_SEPARATOR_STR, time::Duration};

use tempfile::TempDir;

use super::*;
use crate::test_support;
use crate::{Chord, Command, Keymap, PanelView, SortKey, VfsPath};

/// `root/` with `a.txt`, `b.rs`, `sub/c.txt`, `sub/deeper/d.txt` and the empty `empty/`.
fn tree() -> TempDir {
    test_support::tree(&[
        ("empty/", ""),
        ("a.txt", "x"),
        ("b.rs", "x"),
        ("sub/c.txt", "x"),
        ("sub/deeper/d.txt", "x"),
    ])
}

/// A relative name as a flat view spells it on this platform.
fn rel(parts: &[&str]) -> String {
    parts.join(MAIN_SEPARATOR_STR)
}

fn names(state: &PanelState) -> Vec<String> {
    state
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

/// The panel once its flat view has read everything.
async fn settled(engine: &Engine) -> PanelState {
    for _ in 0..500 {
        let state = engine.panel(PanelId::Left);
        if state.flat.as_ref().is_some_and(|flat| !flat.loading) {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the flat view did not finish");
}

fn branch(selection: bool, show: Option<bool>) -> Command {
    Command::BranchView {
        panel: None,
        selection,
        show,
    }
}

#[tokio::test]
async fn the_branch_view_lists_every_file_below_by_its_relative_path() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    engine.dispatch(branch(false, None)).await.expect("branch");
    let state = settled(&engine).await;
    assert_eq!(
        names(&state),
        [
            "a.txt".to_owned(),
            "b.rs".to_owned(),
            rel(&["sub", "c.txt"]),
            rel(&["sub", "deeper", "d.txt"]),
        ]
    );
    assert_eq!(
        state.path,
        VfsPath::local(root.path()),
        "the panel keeps its folder"
    );
    let flat = state.flat.expect("a flat view");
    assert_eq!(flat.source, FlatSource::Branch);
    assert!(!flat.truncated);
    let snapshot = engine.snapshot(None);
    assert_eq!(snapshot.entry_count, 4);
    assert!(snapshot.flat.is_some(), "the panel state says it is flat");
}

#[tokio::test]
async fn the_flat_view_follows_the_panels_sort() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    engine
        .dispatch(Command::Sort {
            panel: None,
            key: SortKey::Ext,
            descending: Some(false),
        })
        .await
        .expect("sort");
    engine
        .dispatch(branch(false, Some(true)))
        .await
        .expect("branch");
    let state = settled(&engine).await;
    assert_eq!(names(&state)[0], "b.rs", "rs before txt");
}

#[tokio::test]
async fn ctrl_shift_b_lists_the_selected_folders_and_files_only() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    engine
        .dispatch(Command::ToggleSelection {
            panel: None,
            names: vec!["sub".into(), "b.rs".into()],
        })
        .await
        .expect("select");
    engine
        .dispatch(branch(true, Some(true)))
        .await
        .expect("branch");
    let state = settled(&engine).await;
    assert_eq!(
        names(&state),
        [
            "b.rs".to_owned(),
            rel(&["sub", "c.txt"]),
            rel(&["sub", "deeper", "d.txt"]),
        ]
    );
    assert_eq!(state.flat.expect("flat").source, FlatSource::Selection);
}

#[tokio::test]
async fn ctrl_b_again_leaves_into_the_folder_of_the_entry_under_the_cursor() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    engine.dispatch(branch(false, None)).await.expect("branch");
    settled(&engine).await;
    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: CursorTarget::Name(rel(&["sub", "deeper", "d.txt"])),
        })
        .await
        .expect("cursor");
    engine.dispatch(branch(false, None)).await.expect("leave");
    let state = engine.panel(PanelId::Left);
    assert_eq!(state.path, VfsPath::local(root.path().join("sub/deeper")));
    assert!(state.flat.is_none());
    assert_eq!(
        state.cursor_entry().map(|entry| entry.name.as_str()),
        Some("d.txt")
    );
}

#[tokio::test]
async fn leaving_an_empty_view_shows_its_own_folder_again() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    engine
        .dispatch(Command::Navigate {
            panel: None,
            path: "empty".into(),
        })
        .await
        .expect("navigate");
    engine.dispatch(branch(false, None)).await.expect("branch");
    assert!(settled(&engine).await.entries.is_empty());
    engine
        .dispatch(branch(false, Some(false)))
        .await
        .expect("leave");
    let state = engine.panel(PanelId::Left);
    assert_eq!(state.path, VfsPath::local(root.path().join("empty")));
    assert!(state.flat.is_none());
}

#[tokio::test]
async fn a_panel_that_moves_on_ends_the_run() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    engine.dispatch(branch(false, None)).await.expect("branch");
    engine
        .dispatch(Command::Navigate {
            panel: None,
            path: "sub".into(),
        })
        .await
        .expect("navigate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let state = engine.panel(PanelId::Left);
    assert!(state.flat.is_none());
    assert_eq!(
        names(&state),
        ["deeper", "c.txt"],
        "the old run wrote nothing"
    );
}

#[tokio::test]
async fn a_refresh_leaves_the_flat_view_for_its_folder() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    engine.dispatch(branch(false, None)).await.expect("branch");
    settled(&engine).await;
    fs::write(root.path().join("new.txt"), b"x").expect("write");
    engine
        .dispatch(Command::Refresh { panel: None })
        .await
        .expect("refresh");
    let state = engine.panel(PanelId::Left);
    assert!(state.flat.is_none(), "the folders are back");
    assert_eq!(state.path, VfsPath::local(root.path()));
    assert!(names(&state).contains(&"sub".to_owned()));
    assert!(names(&state).contains(&"new.txt".to_owned()), "read again");
}

#[cfg(unix)]
#[tokio::test]
async fn hidden_files_and_folders_follow_the_panels_switch() {
    let root = tree();
    fs::create_dir(root.path().join(".git")).expect("mkdir");
    fs::write(root.path().join(".git/config"), b"x").expect("write");
    fs::write(root.path().join("sub/.env"), b"x").expect("write");
    let engine = test_support::headless(root.path()).await;
    engine.dispatch(branch(false, None)).await.expect("branch");
    assert_eq!(settled(&engine).await.entries.len(), 4);
    engine
        .dispatch(Command::ToggleHidden { show: Some(true) })
        .await
        .expect("show hidden");
    let state = settled(&engine).await;
    assert!(names(&state).contains(&rel(&[".git", "config"])));
    assert!(names(&state).contains(&rel(&["sub", ".env"])));
}

#[tokio::test]
async fn fed_paths_show_flat_once_each_and_missing_ones_are_counted() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let elsewhere = tempfile::tempdir().expect("temp dir");
    fs::write(elsewhere.path().join("far.txt"), b"x").expect("write");
    let far = elsewhere.path().join("far.txt");
    engine
        .dispatch(Command::FeedPaths {
            panel: None,
            root: Some("sub".into()),
            paths: vec![
                "c.txt".into(),
                "deeper/d.txt".into(),
                "c.txt".into(),
                "gone.txt".into(),
                far.display().to_string(),
            ],
        })
        .await
        .expect("feed");
    let state = settled(&engine).await;
    assert_eq!(state.path, VfsPath::local(root.path().join("sub")));
    let mut found = names(&state);
    found.sort();
    let mut expected = vec![
        "c.txt".to_owned(),
        rel(&["deeper", "d.txt"]),
        far.display().to_string(),
    ];
    expected.sort();
    assert_eq!(found, expected);
    let flat = state.flat.expect("flat");
    assert_eq!(flat.source, FlatSource::Paths);
    assert_eq!(flat.skipped, 1);
}

#[tokio::test]
async fn feeding_too_many_paths_or_into_no_folder_is_refused() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let error = engine
        .dispatch(Command::FeedPaths {
            panel: None,
            root: None,
            paths: vec!["a.txt".to_owned(); MAX_FLAT_ENTRIES + 1],
        })
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "invalid_search");
    let error = engine
        .dispatch(Command::FeedPaths {
            panel: None,
            root: Some("a.txt".into()),
            paths: Vec::new(),
        })
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "not_a_directory");
    assert!(engine.panel(PanelId::Left).flat.is_none());
}

#[tokio::test]
async fn a_view_stops_at_its_limit_and_says_so() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let mut filler = Filler {
        engine,
        panel: PanelId::Left,
        run: 0,
        found: Vec::new(),
        skipped: 0,
        truncated: false,
        last_flush: Instant::now(),
    };
    let entry = Entry {
        name: "x.txt".to_owned(),
        kind: rc_vfs::EntryKind::File,
        size: 1,
        modified: None,
        created: None,
        attributes: rc_vfs::Attributes::default(),
        hidden: false,
        owner: None,
    };
    for _ in 1..MAX_FLAT_ENTRIES {
        filler.push(entry.clone());
    }
    assert!(!filler.truncated);
    filler.push(entry);
    assert!(filler.truncated);
}

#[tokio::test]
async fn alt_f7_asks_the_window_for_the_find_dialog() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::FindFilesDialog {
            panel: Some(PanelId::Right),
        })
        .await
        .expect("dialog");
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::FindFiles {
            panel: PanelId::Right
        })
    );
}

#[test]
fn the_total_commander_keys_reach_the_flat_view_and_the_dialog() {
    let keymap = Keymap::standard();
    let key = |keys: &str| {
        let chord: Chord = keys.parse().expect("a chord");
        keymap.lookup(&chord, PanelView::Files).cloned()
    };
    assert_eq!(key("Ctrl+B"), Some(branch(false, None)));
    assert_eq!(key("Ctrl+Shift+B"), Some(branch(true, None)));
    assert_eq!(
        key("Alt+F7"),
        Some(Command::FindFilesDialog { panel: None })
    );
}
