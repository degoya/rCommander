//! `workspaces.capture` against the panels and `workspaces.apply` in each mode: locked tabs, a
//! server deleted since, the active panel, the divider, the active workspace and its theme, the
//! start workspace; with a shell, a terminal reused or started.

use std::path::Path;

use rc_vfs::{BackendId, VfsPath};
use serde_json::json;

use super::super::{
    APPLY_MODE_KEY, ON_START_KEY, PanelSpec, SPEC_VERSION, TabSpec, WorkspaceSpec, theme_of,
};
use crate::{
    Command, CreateWorkspaceRequest, Engine, EngineError, LockTabRequest, OpenTabRequest, Outcome,
    PanelId, PanelView, WorkspaceId, WorkspacesOutcome,
    settings::{PANEL_SPLIT_KEY, stored_setting, themes::ThemeMode},
    test_support,
};

fn outcome(result: Result<Outcome, EngineError>) -> WorkspacesOutcome {
    match result.expect("answered") {
        Outcome::Workspaces(outcome) => *outcome,
        other => panic!("a workspaces outcome, got {other:?}"),
    }
}

fn tab(path: &Path, locked: bool) -> TabSpec {
    TabSpec {
        location: VfsPath::local(path),
        sort: None,
        mode: None,
        locked,
        title: None,
    }
}

fn files(tabs: Vec<TabSpec>, active_tab: usize) -> PanelSpec {
    PanelSpec {
        tabs,
        active_tab,
        view: PanelView::Files,
        terminal: None,
    }
}

async fn save(engine: &Engine, name: &str, spec: WorkspaceSpec, dark: Option<&str>) -> WorkspaceId {
    let request = CreateWorkspaceRequest {
        name: name.into(),
        spec,
        theme_light: None,
        theme_dark: dark.map(str::to_owned),
    };
    match outcome(engine.dispatch(Command::CreateWorkspace(request)).await) {
        WorkspacesOutcome::Saved { id } => id,
        other => panic!("saved, got {other:?}"),
    }
}

async fn apply(engine: &Engine, id: WorkspaceId) -> Vec<String> {
    match outcome(engine.dispatch(Command::ApplyWorkspace { id }).await) {
        WorkspacesOutcome::Applied { skipped } => skipped,
        other => panic!("applied, got {other:?}"),
    }
}

/// The tabs of `panel` as (folder, locked, active).
fn tabs_of(engine: &Engine, panel: PanelId) -> Vec<(VfsPath, bool, bool)> {
    engine
        .tabs(panel)
        .into_iter()
        .map(|tab| (tab.path, tab.locked, tab.active))
        .collect()
}

/// An engine on folders `a` to `d` whose left panel has the root and `d`, locked and shown.
async fn with_a_locked_tab() -> (tempfile::TempDir, Engine) {
    let (root, engine) =
        test_support::engine_with_db(&[("a/", ""), ("b/", ""), ("c/", ""), ("d/", "")]).await;
    let open = OpenTabRequest {
        panel: Some(PanelId::Left),
        path: Some("d".into()),
        ..OpenTabRequest::default()
    };
    engine.dispatch(Command::OpenTab(open)).await.expect("open");
    let lock = LockTabRequest {
        panel: Some(PanelId::Left),
        tab: None,
        locked: Some(true),
    };
    engine.dispatch(Command::LockTab(lock)).await.expect("lock");
    (root, engine)
}

/// Left: `a` and `b`, `b` shown; right: `c`; the right panel active, the divider at 400.
fn two_panels(root: &Path) -> WorkspaceSpec {
    WorkspaceSpec {
        version: SPEC_VERSION,
        active: PanelId::Right,
        split_permille: Some(400),
        left: files(
            vec![tab(&root.join("a"), false), tab(&root.join("b"), false)],
            1,
        ),
        right: files(vec![tab(&root.join("c"), false)], 0),
    }
}

async fn applied_in(mode: &str) -> (tempfile::TempDir, Engine, Vec<(VfsPath, bool, bool)>) {
    let (root, engine) = with_a_locked_tab().await;
    let set = Command::SetSetting {
        key: APPLY_MODE_KEY.into(),
        value: json!(mode),
    };
    engine.dispatch(set).await.expect("mode");
    let id = save(&engine, "Two", two_panels(root.path()), None).await;
    assert!(apply(&engine, id).await.is_empty(), "nothing left out");
    let left = tabs_of(&engine, PanelId::Left);
    (root, engine, left)
}

#[tokio::test]
async fn replace_keeps_the_locked_tabs_before_the_workspaces() {
    let (root, engine, left) = applied_in("replace_keep_locked").await;
    let at = |name: &str| VfsPath::local(root.path().join(name));
    assert_eq!(
        left,
        [
            (at("d"), true, false),
            (at("a"), false, false),
            (at("b"), false, true)
        ]
    );
    assert_eq!(
        engine.panel(PanelId::Left).path,
        at("b"),
        "the workspace's tab is shown"
    );
    assert_eq!(tabs_of(&engine, PanelId::Right), [(at("c"), false, true)]);
    assert_eq!(engine.active_panel(), PanelId::Right);
    let db = engine.db().expect("db");
    assert_eq!(stored_setting(db, PANEL_SPLIT_KEY).await, Some(json!(400)));
}

#[tokio::test]
async fn replace_all_takes_the_locked_tabs_too() {
    let (root, _engine, left) = applied_in("replace_all").await;
    let at = |name: &str| VfsPath::local(root.path().join(name));
    assert_eq!(left, [(at("a"), false, false), (at("b"), false, true)]);
}

#[tokio::test]
async fn add_puts_the_workspaces_tabs_after_the_panels() {
    let (root, _engine, left) = applied_in("add").await;
    let at = |name: &str| VfsPath::local(root.path().join(name));
    assert_eq!(
        left,
        [
            (VfsPath::local(root.path()), false, false),
            (at("d"), true, false),
            (at("a"), false, false),
            (at("b"), false, true),
        ]
    );
}

#[tokio::test]
async fn a_deleted_server_is_left_out_with_a_note_and_the_rest_applies() {
    let (root, engine) = test_support::engine_with_db(&[("a/", "")]).await;
    let draft = crate::ConnectionDraft::new("nas", "nas.example.org");
    let created = Command::CreateConnection {
        folder_id: None,
        connection: draft,
    };
    let server = match engine.dispatch(created).await.expect("created") {
        Outcome::Connection { connection } => connection.id,
        other => panic!("a connection, got {other:?}"),
    };
    let mut spec = two_panels(root.path());
    spec.left.tabs = vec![tab(&root.path().join("a"), false)];
    spec.left.active_tab = 0;
    spec.right.tabs = vec![TabSpec {
        location: VfsPath {
            backend: BackendId::Ftp {
                connection_id: server.0,
            },
            path: "/pub".into(),
        },
        sort: None,
        mode: None,
        locked: false,
        title: None,
    }];
    let id = save(&engine, "Nas", spec, None).await;
    engine
        .dispatch(Command::DeleteConnection { id: server })
        .await
        .expect("deleted");
    let skipped = apply(&engine, id).await;
    assert_eq!(
        skipped,
        [format!("workspace_connection:ftp://#{}/pub", server.0)]
    );
    let at = VfsPath::local(root.path().join("a"));
    assert_eq!(tabs_of(&engine, PanelId::Left), [(at, false, true)]);
    assert_eq!(
        tabs_of(&engine, PanelId::Right),
        [(VfsPath::local(root.path()), false, true)],
        "a panel with nothing of the workspace left stays as it was"
    );
    assert_eq!(engine.active_panel(), PanelId::Right);
}

#[tokio::test]
async fn the_applied_workspace_is_active_and_its_theme_wins_until_it_is_deleted() {
    let (root, engine) = test_support::engine_with_db(&[("a/", ""), ("b/", ""), ("c/", "")]).await;
    let first = save(&engine, "One", two_panels(root.path()), Some("fluent-dark")).await;
    let second = save(&engine, "Two", two_panels(root.path()), None).await;
    let mut events = engine.subscribe();
    apply(&engine, first).await;
    assert!(
        std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| event == crate::EngineEvent::WorkspacesChanged)
    );
    assert_eq!(engine.active_workspace(), Some(first));
    let list = engine.workspaces().await.expect("list");
    assert_eq!(
        list.iter()
            .filter(|w| w.active)
            .map(|w| w.id)
            .collect::<Vec<_>>(),
        [first]
    );
    assert_eq!(theme_of(&list, ThemeMode::Dark), Some("fluent-dark"));
    assert_eq!(theme_of(&list, ThemeMode::Light), None);
    apply(&engine, second).await;
    let list = engine.workspaces().await.expect("list");
    assert_eq!(theme_of(&list, ThemeMode::Dark), None, "the window's again");
    engine
        .dispatch(Command::DeleteWorkspace { id: second })
        .await
        .expect("deleted");
    assert_eq!(engine.active_workspace(), None);
}

#[tokio::test]
async fn the_start_workspace_is_the_settings() {
    let (root, engine) = test_support::engine_with_db(&[("a/", ""), ("b/", ""), ("c/", "")]).await;
    let id = save(&engine, "Start", two_panels(root.path()), None).await;
    let set = Command::SetSetting {
        key: ON_START_KEY.into(),
        value: json!(id.to_string()),
    };
    engine.dispatch(set).await.expect("set");
    let start = engine.start_workspace().await.expect("one");
    assert!(apply(&engine, start).await.is_empty());
    assert_eq!(
        engine.panel(PanelId::Right).path,
        VfsPath::local(root.path().join("c"))
    );
}

#[tokio::test]
async fn capture_takes_tabs_view_active_panel_and_divider() {
    let (root, engine) = with_a_locked_tab().await;
    let split = Command::SetSetting {
        key: PANEL_SPLIT_KEY.into(),
        value: json!(650),
    };
    engine.dispatch(split).await.expect("split");
    let switch = Command::SwitchActivePanel {
        panel: Some(PanelId::Right),
    };
    engine.dispatch(switch).await.expect("switch");
    let named = Command::CaptureWorkspace {
        id: None,
        name: Some("Now".into()),
    };
    let id = match outcome(engine.dispatch(named).await) {
        WorkspacesOutcome::Saved { id } => id,
        other => panic!("saved, got {other:?}"),
    };
    let spec = engine.read_workspace(id).await.expect("read").spec;
    assert_eq!(spec.active, PanelId::Right);
    assert_eq!(spec.split_permille, Some(650));
    let left: Vec<(VfsPath, bool)> = spec
        .left
        .tabs
        .iter()
        .map(|tab| (tab.location.clone(), tab.locked))
        .collect();
    assert_eq!(
        left,
        [
            (VfsPath::local(root.path()), false),
            (VfsPath::local(root.path().join("d")), true),
        ]
    );
    assert_eq!(spec.left.active_tab, 1);
    assert_eq!(spec.left.view, PanelView::Files);
    assert_eq!(spec.right.tabs.len(), 1);

    engine
        .dispatch(Command::SwitchActivePanel {
            panel: Some(PanelId::Left),
        })
        .await
        .expect("switch");
    let over = Command::CaptureWorkspace {
        id: Some(id),
        name: None,
    };
    assert_eq!(
        outcome(engine.dispatch(over).await),
        WorkspacesOutcome::Saved { id }
    );
    let again = engine.read_workspace(id).await.expect("read");
    assert_eq!(again.name, "Now");
    assert_eq!(again.spec.active, PanelId::Left);
    let unnamed = Command::CaptureWorkspace {
        id: None,
        name: None,
    };
    let refused = engine.dispatch(unnamed).await.expect_err("refused");
    assert_eq!(refused.code(), "workspace_name");
}

#[cfg(unix)]
mod with_a_shell {
    use super::*;
    use crate::workspaces::TermSpec;

    fn terminal(root: &Path, shell: &str) -> WorkspaceSpec {
        let mut spec = two_panels(root);
        spec.left.view = PanelView::Terminal;
        spec.left.terminal = Some(TermSpec::Local {
            shell: Some(shell.into()),
            cwd: None,
        });
        spec
    }

    #[tokio::test]
    async fn a_terminal_is_started_then_reused_and_another_kind_is_never_taken_for_it() {
        let (root, engine) =
            test_support::engine_with_db(&[("a/", ""), ("b/", ""), ("c/", "")]).await;
        let sh = save(&engine, "Shell", terminal(root.path(), "sh"), None).await;
        let plain = save(&engine, "Files", two_panels(root.path()), None).await;
        assert!(apply(&engine, sh).await.is_empty());
        assert_eq!(engine.panel(PanelId::Left).view, PanelView::Terminal);
        let started = engine.panel_terminals(PanelId::Left);
        assert_eq!(started.len(), 1, "{started:?}");
        assert_eq!(
            started[0].cwd,
            root.path().join("b"),
            "in the shown tab's folder"
        );

        apply(&engine, plain).await;
        assert_eq!(engine.panel(PanelId::Left).view, PanelView::Files);
        apply(&engine, sh).await;
        let reused = engine.panel_terminals(PanelId::Left);
        assert_eq!(reused.len(), 1, "the running shell is reused: {reused:?}");
        assert_eq!(
            engine.shown_terminal(PanelId::Left),
            Some(started[0].terminal)
        );
        assert!(engine.panel_terminals(PanelId::Right).is_empty());

        let other = save(
            &engine,
            "Other",
            terminal(root.path(), "no-such-shell"),
            None,
        )
        .await;
        let skipped = apply(&engine, other).await;
        assert_eq!(skipped.len(), 1, "{skipped:?}");
        assert!(skipped[0].ends_with(":local:no-such-shell"), "{skipped:?}");
        assert_eq!(engine.panel(PanelId::Left).view, PanelView::Files);
        assert_eq!(engine.panel_terminals(PanelId::Left).len(), 1);
        for info in engine.panel_terminals(PanelId::Left) {
            let _ = engine
                .dispatch(Command::CloseTerminal {
                    terminal: info.terminal,
                })
                .await;
        }
    }
}

#[path = "apply_switch_tests.rs"]
mod switching;

/// RC-100-92: a tab's own name goes into a workspace and comes back with it; a bad one is refused.
#[tokio::test]
async fn a_workspace_carries_tab_names() {
    let (root, engine) = test_support::engine_with_db(&[("a/", ""), ("b/", ""), ("c/", "")]).await;
    let mut spec = two_panels(root.path());
    spec.left.tabs[0].title = Some("Alpha".into());
    let id = save(&engine, "Named", spec.clone(), None).await;
    assert!(apply(&engine, id).await.is_empty());
    let titles: Vec<(String, bool)> = engine
        .tabs(PanelId::Left)
        .into_iter()
        .map(|tab| (tab.title, tab.renamed))
        .collect();
    assert_eq!(
        titles,
        [("Alpha".to_owned(), true), ("b".to_owned(), false)]
    );
    spec.left.tabs[0].title = Some("x".repeat(65));
    let request = CreateWorkspaceRequest {
        name: "Bad".into(),
        spec,
        theme_light: None,
        theme_dark: None,
    };
    assert!(
        engine
            .dispatch(Command::CreateWorkspace(request))
            .await
            .is_err()
    );
}
