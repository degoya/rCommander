use tempfile::TempDir;

use super::*;
use crate::test_support;
use crate::test_support::expect_tabs;
use crate::{
    Chord, Command, CursorTarget, Keymap, PanelView, SortKey, TabView,
    tabs::requests::{
        LockTabRequest, MoveTabRequest, NextTabRequest, OpenTabRequest, PanelRequest,
        SelectTabRequest, TabRequest,
    },
};

async fn engine() -> (TempDir, Engine) {
    test_support::engine(&[
        ("alpha/inner/", ""),
        ("beta/", ""),
        ("notes.txt", "x"),
        ("alpha/readme.md", "x"),
    ])
    .await
}

async fn run(engine: &Engine, command: Command) -> Outcome {
    engine.dispatch(command).await.expect("command")
}

async fn list(engine: &Engine, panel: PanelId) -> Vec<TabView> {
    expect_tabs(
        run(
            engine,
            Command::ListTabs(PanelRequest { panel: Some(panel) }),
        )
        .await,
    )
}

async fn go(engine: &Engine, panel: PanelId, path: &str) {
    run(
        engine,
        Command::Navigate {
            panel: Some(panel),
            path: path.into(),
        },
    )
    .await;
}

fn name(engine: &Engine, panel: PanelId) -> String {
    engine
        .panel(panel)
        .path
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn new_tab() -> Command {
    Command::OpenTab(OpenTabRequest::default())
}

/// Ctrl+T opens a tab on the panel's folder and shows it; the first tab keeps its own.
#[tokio::test]
async fn a_new_tab_starts_on_the_panels_folder_and_is_shown() {
    let (_root, engine) = engine().await;
    let before = list(&engine, PanelId::Left).await;
    assert_eq!(before.len(), 1);
    go(&engine, PanelId::Left, "alpha").await;
    let tabs = expect_tabs(run(&engine, new_tab()).await);
    assert_eq!(tabs.len(), 2);
    assert!(tabs[1].active && !tabs[0].active);
    assert_eq!(tabs[0].path, tabs[1].path);
    // Two tabs on one folder are told apart.
    assert!(tabs[0].title.starts_with("alpha (") && tabs[0].title != tabs[1].title);
    assert_eq!(
        list(&engine, PanelId::Right).await.len(),
        1,
        "the other panel"
    );
}

/// The acceptance check of RC-010-03: Ctrl+W closes, the last tab stays.
#[tokio::test]
async fn the_last_tab_is_never_closed() {
    let (_root, engine) = engine().await;
    run(&engine, new_tab()).await;
    let close = || Command::CloseTab(TabRequest::default());
    assert_eq!(expect_tabs(run(&engine, close()).await).len(), 1);
    let error = engine.dispatch(close()).await.expect_err("last tab");
    assert_eq!(error.code(), "last_tab");
}

/// Ctrl+Tab and Ctrl+Shift+Tab step through the tabs, round at the ends; each tab keeps its
/// folder, sort and cursor.
#[tokio::test]
async fn tabs_keep_folder_sort_and_cursor() {
    let (_root, engine) = engine().await;
    run(
        &engine,
        Command::SetCursor {
            panel: None,
            target: CursorTarget::Name("beta".into()),
        },
    )
    .await;
    run(&engine, new_tab()).await;
    go(&engine, PanelId::Left, "alpha").await;
    run(
        &engine,
        Command::Sort {
            panel: None,
            key: SortKey::Size,
            descending: None,
        },
    )
    .await;
    let next = |backwards| {
        Command::NextTab(NextTabRequest {
            panel: None,
            backwards,
        })
    };
    run(&engine, next(false)).await;
    let first = engine.panel(PanelId::Left);
    assert_ne!(name(&engine, PanelId::Left), "alpha");
    assert_eq!(first.sort, SortOrder::default());
    assert_eq!(
        first.cursor_entry().map(|entry| entry.name.as_str()),
        Some("beta")
    );
    run(&engine, next(true)).await;
    assert_eq!(name(&engine, PanelId::Left), "alpha");
    assert_eq!(engine.panel(PanelId::Left).sort.key, SortKey::Size);
}

#[tokio::test]
async fn closing_the_shown_tab_shows_its_right_neighbour() {
    let (_root, engine) = engine().await;
    run(&engine, new_tab()).await;
    go(&engine, PanelId::Left, "alpha").await;
    run(&engine, new_tab()).await;
    go(&engine, PanelId::Left, "inner").await;
    let tabs = list(&engine, PanelId::Left).await;
    run(
        &engine,
        Command::SelectTab(SelectTabRequest {
            panel: None,
            tab: tabs[1].id,
        }),
    )
    .await;
    let after = expect_tabs(run(&engine, Command::CloseTab(TabRequest::default())).await);
    assert_eq!(after.len(), 2);
    assert!(after[1].active);
    assert_eq!(name(&engine, PanelId::Left), "inner");
}

/// The acceptance check of RC-010-03: a locked tab opens a subfolder in a new tab and keeps its
/// own folder.
#[tokio::test]
async fn a_locked_tab_opens_a_subfolder_in_a_new_tab() {
    let (_root, engine) = engine().await;
    let locked = expect_tabs(
        run(
            &engine,
            Command::LockTab(LockTabRequest {
                locked: Some(true),
                ..LockTabRequest::default()
            }),
        )
        .await,
    );
    assert!(locked[0].locked);
    go(&engine, PanelId::Left, "alpha").await;
    let tabs = list(&engine, PanelId::Left).await;
    assert_eq!(tabs.len(), 2);
    assert!(tabs[0].locked && !tabs[1].locked);
    assert!(tabs[1].active);
    assert_eq!(name(&engine, PanelId::Left), "alpha");
    assert_ne!(tabs[0].path, tabs[1].path, "the locked tab kept its folder");
    // Back on the locked tab, its own folder again is no new tab.
    run(
        &engine,
        Command::SelectTab(SelectTabRequest {
            panel: None,
            tab: tabs[0].id,
        }),
    )
    .await;
    go(&engine, PanelId::Left, ".").await;
    assert_eq!(list(&engine, PanelId::Left).await.len(), 2);
}

/// Ctrl+Up opens the folder under the cursor in a new tab, Ctrl+Shift+Up in a new tab of the
/// other panel; on a file it is the panel's own folder.
#[tokio::test]
async fn the_folder_under_the_cursor_opens_in_a_new_tab() {
    let (_root, engine) = engine().await;
    run(
        &engine,
        Command::SetCursor {
            panel: None,
            target: CursorTarget::Name("alpha".into()),
        },
    )
    .await;
    let open = |other| {
        Command::OpenTab(OpenTabRequest {
            cursor: true,
            other,
            ..OpenTabRequest::default()
        })
    };
    let right = expect_tabs(run(&engine, open(true)).await);
    assert_eq!(right.len(), 2);
    assert_eq!(right[1].title, "alpha");
    assert_eq!(name(&engine, PanelId::Right), "alpha");
    assert_eq!(list(&engine, PanelId::Left).await.len(), 1);
    let left = expect_tabs(run(&engine, open(false)).await);
    assert_eq!(left[1].title, "alpha");
    run(
        &engine,
        Command::SetCursor {
            panel: Some(PanelId::Left),
            target: CursorTarget::Name("readme.md".into()),
        },
    )
    .await;
    let on_file = expect_tabs(run(&engine, open(false)).await);
    assert_eq!(on_file[2].path, on_file[1].path, "the panel's own folder");
}

#[tokio::test]
async fn a_copied_tab_lands_unlocked_behind_the_other_panels_tabs() {
    let (_root, engine) = engine().await;
    go(&engine, PanelId::Left, "beta").await;
    run(&engine, Command::LockTab(LockTabRequest::default())).await;
    let right = expect_tabs(run(&engine, Command::CopyTabToOther(TabRequest::default())).await);
    assert_eq!(right.len(), 2);
    assert_eq!(right[1].title, "beta");
    assert!(!right[1].locked && !right[1].active);
    assert_ne!(
        name(&engine, PanelId::Right),
        "beta",
        "shown in the background"
    );
}

fn move_tab(tab: TabId, to: usize) -> Command {
    Command::MoveTab(MoveTabRequest {
        panel: Some(PanelId::Left),
        tab,
        to,
    })
}

fn titles(tabs: &[TabView]) -> Vec<&str> {
    tabs.iter().map(|tab| tab.title.as_str()).collect()
}

/// A tab dragged to another place (RC-010-24): the row's order changes, the panel keeps showing
/// the tab it showed, and the other panel's row stays as it is.
#[tokio::test]
async fn a_moved_tab_takes_its_new_place_and_the_shown_tab_stays_shown() {
    let (root, engine) = engine().await;
    run(&engine, new_tab()).await;
    go(&engine, PanelId::Left, "alpha").await;
    run(&engine, new_tab()).await;
    go(&engine, PanelId::Left, "../beta").await;
    let before = list(&engine, PanelId::Left).await;
    let first = root.path().file_name().and_then(|name| name.to_str());
    let first = first.expect("the folder's name");
    assert_eq!(titles(&before), [first, "alpha", "beta"]);

    let moved = expect_tabs(run(&engine, move_tab(before[0].id, 2)).await);
    assert_eq!(titles(&moved), ["alpha", "beta", first]);
    assert!(moved[1].active, "beta is still the shown tab");
    assert_eq!(name(&engine, PanelId::Left), "beta");
    assert_eq!(list(&engine, PanelId::Right).await.len(), 1);

    // To the front, past the shown tab.
    let back = expect_tabs(run(&engine, move_tab(before[2].id, 0)).await);
    assert_eq!(titles(&back), ["beta", "alpha", first]);
    assert!(back[0].active);
    // The shown tab is still the one shown after the move, so stepping goes on from there.
    run(&engine, Command::NextTab(NextTabRequest::default())).await;
    assert_eq!(name(&engine, PanelId::Left), "alpha");
}

/// A place beyond the row is its last one; a tab of the other panel, or none, is `tab_not_found`.
#[tokio::test]
async fn a_move_beyond_the_row_goes_last_and_an_unknown_tab_is_named() {
    let (_root, engine) = engine().await;
    run(&engine, new_tab()).await;
    go(&engine, PanelId::Left, "alpha").await;
    let before = list(&engine, PanelId::Left).await;
    let moved = expect_tabs(run(&engine, move_tab(before[0].id, 99)).await);
    assert_eq!(moved[1].id, before[0].id);
    assert_eq!(moved[0].id, before[1].id);

    let right = list(&engine, PanelId::Right).await[0].id;
    let error = engine
        .dispatch(move_tab(right, 0))
        .await
        .expect_err("not a tab of the left panel");
    assert_eq!(error.code(), "tab_not_found");
}

#[tokio::test]
async fn an_unknown_tab_is_named() {
    let (_root, engine) = engine().await;
    let error = engine
        .dispatch(Command::SelectTab(SelectTabRequest {
            panel: None,
            tab: TabId(999),
        }))
        .await
        .expect_err("unknown");
    assert_eq!(error.code(), "tab_not_found");
}

/// A tab whose folder was removed meanwhile shows the nearest folder above it.
#[tokio::test]
async fn a_tab_on_a_removed_folder_shows_its_parent() {
    let (root, engine) = engine().await;
    go(&engine, PanelId::Left, "alpha/inner").await;
    run(&engine, new_tab()).await;
    go(&engine, PanelId::Left, "/").await;
    std::fs::remove_dir(root.path().join("alpha").join("inner")).expect("remove");
    let first = list(&engine, PanelId::Left).await[0].id;
    let tabs = expect_tabs(
        run(
            &engine,
            Command::SelectTab(SelectTabRequest {
                panel: None,
                tab: first,
            }),
        )
        .await,
    );
    assert_eq!(tabs[0].title, "alpha");
    assert_eq!(name(&engine, PanelId::Left), "alpha");
}

/// The Total Commander keys of the tabs and the history run their commands.
#[test]
fn the_total_commander_keys_run_the_tab_commands() {
    let keymap = Keymap::standard();
    let files = |keys: &str| {
        let chord: Chord = keys.parse().expect("chord");
        keymap.lookup(&chord, PanelView::Files).map(Command::id)
    };
    for (keys, id) in [
        ("Ctrl+T", "tabs.open"),
        ("Ctrl+ArrowUp", "tabs.open"),
        ("Ctrl+Shift+ArrowUp", "tabs.open"),
        ("Ctrl+W", "tabs.close"),
        ("Ctrl+Tab", "tabs.next"),
        ("Ctrl+Shift+Tab", "tabs.next"),
        ("Alt+ArrowLeft", "history.back"),
        ("Alt+ArrowRight", "history.forward"),
        ("Alt+ArrowDown", "history.list"),
        ("Ctrl+D", "hotlist.list"),
        ("Ctrl+U", "panel.swap"),
        ("Ctrl+Shift+U", "panel.swap"),
        ("Ctrl+ArrowLeft", "panel.transfer"),
        ("Ctrl+ArrowRight", "panel.transfer"),
    ] {
        assert_eq!(files(keys), Some(id), "{keys}");
    }
}
