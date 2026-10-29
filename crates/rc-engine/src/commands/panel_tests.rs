use std::path::PathBuf;

use super::resolve_path;
use crate::{
    Command, CursorStep, PanelId, VfsPath,
    test_support::{self, run_panel},
};

fn at(path: &str) -> VfsPath {
    VfsPath::local(path)
}

#[test]
fn a_relative_target_joins_and_dots_fold() {
    assert_eq!(resolve_path(&at("/a/b"), "c").path, PathBuf::from("/a/b/c"));
    assert_eq!(resolve_path(&at("/a/b"), "..").path, PathBuf::from("/a"));
    assert_eq!(
        resolve_path(&at("/a/b"), "./../c/./d").path,
        PathBuf::from("/a/c/d")
    );
}

#[test]
fn an_absolute_target_replaces_and_the_root_has_no_parent() {
    assert_eq!(
        resolve_path(&at("/a/b"), "/x/y").path,
        PathBuf::from("/x/y")
    );
    assert_eq!(resolve_path(&at("/"), "..").path, PathBuf::from("/"));
    assert_eq!(resolve_path(&at("/a"), "../../..").path, PathBuf::from("/"));
}

/// Ctrl+\ navigates to `/`: the root of the panel's file system (on Windows of its drive or
/// share).
#[test]
fn the_root_target_is_the_root_of_the_current_path() {
    assert_eq!(resolve_path(&at("/a/b"), "/").path, PathBuf::from("/"));
}

#[cfg(windows)]
#[test]
fn a_root_without_a_drive_stays_on_the_drive_or_share() {
    assert_eq!(
        resolve_path(&at(r"D:\work\src"), "\\").path,
        PathBuf::from(r"D:\")
    );
    assert_eq!(
        resolve_path(&at(r"\\wsl.localhost\Ubuntu\home"), "/").path,
        PathBuf::from(r"\\wsl.localhost\Ubuntu\")
    );
}

/// F5 from an FTP or SFTP panel into a drive or a share copies to this machine (owner's report
/// 2026-10-18: FTP -> `D:\` failed with "This path is not valid here"); a server path stays on
/// the server.
#[cfg(windows)]
#[test]
fn a_drive_or_share_typed_on_a_server_panel_is_local() {
    use rc_vfs::BackendId;
    for backend in [
        BackendId::Ftp { connection_id: 7 },
        BackendId::Sftp { connection_id: 7 },
    ] {
        let server = VfsPath {
            backend: backend.clone(),
            path: PathBuf::from("/home/tester"),
        };
        let drive = resolve_path(&server, r"D:\");
        assert_eq!(drive.backend, BackendId::Local);
        assert_eq!(drive.path, PathBuf::from(r"D:\"));
        let share = resolve_path(&server, r"\\nas\media\in");
        assert_eq!(share.backend, BackendId::Local);
        assert_eq!(share.path, PathBuf::from(r"\\nas\media\in"));
        assert_eq!(resolve_path(&server, "/srv").backend, backend);
    }
}

fn toggle(names: &[&str]) -> Command {
    Command::ToggleSelection {
        panel: None,
        names: names.iter().map(|name| (*name).to_owned()).collect(),
    }
}

/// RC-080-45: a cursor step answers the panel's own selection, no copy of it, and the copy the
/// window takes of the state shares it too.
#[tokio::test]
async fn a_cursor_step_answers_the_panels_own_selection() {
    let root = test_support::tree(&[("a.txt", "a"), ("b.txt", "b"), ("c.txt", "c")]);
    let engine = test_support::headless(root.path()).await;
    run_panel(&engine, toggle(&["a.txt", "b.txt"])).await;
    let stepped = run_panel(
        &engine,
        Command::MoveCursor {
            panel: None,
            step: CursorStep::Down,
        },
    )
    .await;
    assert_eq!(stepped.selection, ["a.txt", "b.txt"]);
    let state = engine.panel(PanelId::Left);
    assert!(stepped.selection.shares(&state.selection));
    assert_eq!(engine.panel_path(PanelId::Left), state.path);
}

/// RC-080-45: a refresh keeps the selected names still listed - the same set when none went -
/// and the cursor on its entry.
#[tokio::test]
async fn a_refresh_keeps_the_selected_names_that_are_still_listed() {
    let root = test_support::tree(&[("a.txt", "a"), ("b.txt", "b"), ("c.txt", "c")]);
    let engine = test_support::headless(root.path()).await;
    run_panel(&engine, toggle(&["a.txt", "c.txt"])).await;
    let before = engine.panel(PanelId::Left).selection;
    let again = run_panel(&engine, Command::Refresh { panel: None }).await;
    assert!(again.selection.shares(&before), "nothing went: no new set");
    std::fs::remove_file(root.path().join("c.txt")).expect("remove");
    run_panel(
        &engine,
        Command::SetCursor {
            panel: None,
            target: crate::CursorTarget::Name("b.txt".into()),
        },
    )
    .await;
    let reread = run_panel(&engine, Command::Refresh { panel: None }).await;
    assert_eq!(reread.selection, ["a.txt"]);
    assert_eq!(
        reread.cursor.map(|cursor| cursor.name).as_deref(),
        Some("b.txt")
    );
}

/// One unknown name toggles nothing, as one of several does.
#[tokio::test]
async fn one_unknown_name_toggles_nothing() {
    let root = test_support::tree(&[("a.txt", "a")]);
    let engine = test_support::headless(root.path()).await;
    let error = engine
        .dispatch(toggle(&["ghost"]))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "entry_not_found");
    let error = engine
        .dispatch(toggle(&["a.txt", "ghost"]))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "entry_not_found");
    assert!(engine.panel(PanelId::Left).selection.is_empty());
}
