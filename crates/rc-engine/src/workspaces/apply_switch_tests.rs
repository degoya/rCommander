//! RC-100-72: two workspaces called up in turn in each mode of `workspaces.apply_mode` - the tabs
//! of a panel never grow, a workspace from `workspaces.capture` with a locked tab included - and
//! the merge that opens a folder the panel shows already no second time.

use super::super::{merged, new_tab};
use super::*;
use crate::{ListMode, SortOrder};

/// A panel's tabs as (folder, locked, active).
type Shown = Vec<(VfsPath, bool, bool)>;

/// How often each workspace is called up.
const ROUNDS: usize = 10;

async fn set_mode(engine: &Engine, mode: &str) {
    let set = Command::SetSetting {
        key: APPLY_MODE_KEY.into(),
        value: json!(mode),
    };
    engine.dispatch(set).await.expect("mode");
}

/// Folders `a` to `d`, and two workspaces. A: left `a` locked and `b`, `b` shown; right `c`.
/// B: left `c` and `d`, `c` shown; right `a`.
async fn with_two() -> (tempfile::TempDir, Engine, WorkspaceId, WorkspaceId) {
    let (root, engine) =
        test_support::engine_with_db(&[("a/", ""), ("b/", ""), ("c/", ""), ("d/", "")]).await;
    let at = |name: &str| root.path().join(name);
    let mut first = two_panels(root.path());
    first.left.tabs[0].locked = true;
    let mut second = two_panels(root.path());
    second.left = files(vec![tab(&at("c"), false), tab(&at("d"), false)], 0);
    second.right = files(vec![tab(&at("a"), false)], 0);
    let first = save(&engine, "A", first, None).await;
    let second = save(&engine, "B", second, None).await;
    (root, engine, first, second)
}

/// Both panels' tabs after each call: `first`, then `second`, `ROUNDS` times.
async fn in_turn(engine: &Engine, first: WorkspaceId, second: WorkspaceId) -> Vec<[Shown; 2]> {
    let mut seen = Vec::new();
    for _ in 0..ROUNDS {
        for id in [first, second] {
            assert!(apply(engine, id).await.is_empty(), "nothing left out");
            seen.push([
                tabs_of(engine, PanelId::Left),
                tabs_of(engine, PanelId::Right),
            ]);
        }
    }
    seen
}

/// Every call of `seen` left the panels as `first` (even calls) or `second` (odd ones) says.
fn alternates(seen: &[[Shown; 2]], first: &[Shown; 2], second: &[Shown; 2]) {
    for (call, panels) in seen.iter().enumerate() {
        let wanted = if call % 2 == 0 { first } else { second };
        assert_eq!(panels, wanted, "call {call}");
    }
}

#[tokio::test]
async fn by_default_each_panel_shows_exactly_the_workspaces_tabs() {
    let (root, engine, first, second) = with_two().await;
    let at = |name: &str| VfsPath::local(root.path().join(name));
    let seen = in_turn(&engine, first, second).await;
    let a = [
        vec![(at("a"), true, false), (at("b"), false, true)],
        vec![(at("c"), false, true)],
    ];
    let b = [
        vec![(at("c"), false, true), (at("d"), false, false)],
        vec![(at("a"), false, true)],
    ];
    alternates(&seen, &a, &b);
}

#[tokio::test]
async fn keeping_locked_tabs_never_opens_a_kept_folder_again() {
    let (root, engine, first, second) = with_two().await;
    set_mode(&engine, "replace_keep_locked").await;
    let at = |name: &str| VfsPath::local(root.path().join(name));
    let seen = in_turn(&engine, first, second).await;
    let a = [
        vec![(at("a"), true, false), (at("b"), false, true)],
        vec![(at("c"), false, true)],
    ];
    let b = [
        vec![
            (at("a"), true, false),
            (at("c"), false, true),
            (at("d"), false, false),
        ],
        vec![(at("a"), false, true)],
    ];
    alternates(&seen, &a, &b);
}

#[tokio::test]
async fn adding_grows_once_and_then_shows_the_open_tabs() {
    let (root, engine, first, second) = with_two().await;
    set_mode(&engine, "add").await;
    let at = |name: &str| VfsPath::local(root.path().join(name));
    let top = VfsPath::local(root.path());
    let seen = in_turn(&engine, first, second).await;
    assert_eq!(
        seen[0],
        [
            vec![
                (top.clone(), false, false),
                (at("a"), true, false),
                (at("b"), false, true),
            ],
            vec![(top.clone(), false, false), (at("c"), false, true)],
        ]
    );
    let all = |shown: &str| {
        let mut tabs = vec![(top.clone(), false, false)];
        tabs.extend(["a", "b", "c", "d"].map(|name| (at(name), name == "a", name == shown)));
        tabs
    };
    let right = |shown: &str| {
        vec![
            (top.clone(), false, false),
            (at("c"), false, shown == "c"),
            (at("a"), false, shown == "a"),
        ]
    };
    alternates(&seen[1..], &[all("c"), right("a")], &[all("b"), right("c")]);
}

#[tokio::test]
async fn a_captured_workspace_with_a_locked_tab_does_not_grow_in_either_replace_mode() {
    for mode in ["replace_all", "replace_keep_locked"] {
        let (root, engine) = with_a_locked_tab().await;
        let at = |name: &str| VfsPath::local(root.path().join(name));
        let top = VfsPath::local(root.path());
        let named = Command::CaptureWorkspace {
            id: None,
            name: Some("Captured".into()),
        };
        let captured = match outcome(engine.dispatch(named).await) {
            WorkspacesOutcome::Saved { id } => id,
            other => panic!("saved, got {other:?}"),
        };
        let mut other = two_panels(root.path());
        other.left = files(vec![tab(&root.path().join("a"), false)], 0);
        let other = save(&engine, "Other", other, None).await;
        set_mode(&engine, mode).await;
        let seen = in_turn(&engine, captured, other).await;
        let left = if mode == "replace_all" {
            vec![(top.clone(), false, false), (at("d"), true, true)]
        } else {
            vec![(at("d"), true, true), (top.clone(), false, false)]
        };
        let mut away = vec![(at("a"), false, true)];
        if mode == "replace_keep_locked" {
            away.insert(0, (at("d"), true, false));
        }
        let back = [left, vec![(top.clone(), false, true)]];
        alternates(&seen, &back, &[away, vec![(at("c"), false, true)]]);
    }
}

#[test]
fn a_folder_shown_already_stands_for_the_workspaces_tab() {
    let listing = (SortOrder::default(), ListMode::default());
    let open = |name: &str| new_tab(&tab(Path::new(name), false), listing);
    let names = |tabs: &[crate::tabs::Tab]| {
        tabs.iter()
            .map(|each| each.path.path.display().to_string())
            .collect::<Vec<_>>()
    };
    let wanted = || vec![open("y"), open("x"), open("y")];

    let (tabs, shown) = merged(vec![open("x")], wanted(), 1);
    assert_eq!(names(&tabs), ["x", "y"]);
    assert_eq!(shown, 0, "the kept tab on the workspace's active folder");
    let (tabs, shown) = merged(vec![open("x")], wanted(), 2);
    assert_eq!((names(&tabs).len(), shown), (2, 1));
    let (tabs, shown) = merged(Vec::new(), wanted(), 1);
    assert_eq!(names(&tabs), ["y", "x"], "the workspace's own twice: once");
    assert_eq!(shown, 1);
}
