use rc_db::Db;
use tempfile::TempDir;

use super::*;
use crate::test_support;
use crate::{
    Command, FileFilter, LockTabRequest, PanelRequest, TabView, tabs::requests::OpenTabRequest,
};

/// A folder with `a` and `b`.
fn folder() -> TempDir {
    let root = tempfile::tempdir().expect("temp dir");
    for dir in ["a", "b"] {
        std::fs::create_dir_all(root.path().join(dir)).expect("dir");
    }
    root
}

async fn start(root: &TempDir, db: &Db) -> Engine {
    let engine = test_support::headless(root.path()).await;
    assert!(engine.attach_db(db.clone()));
    engine
}

async fn run(engine: &Engine, command: Command) -> Outcome {
    engine.dispatch(command).await.expect("command")
}

async fn tabs(engine: &Engine) -> Vec<TabView> {
    test_support::expect_tabs(
        run(
            engine,
            Command::ListTabs(PanelRequest {
                panel: Some(PanelId::Left),
            }),
        )
        .await,
    )
}

fn rename(title: Option<&str>) -> Command {
    Command::RenameTab {
        panel: None,
        tab: None,
        title: title.map(str::to_owned),
    }
}

#[tokio::test]
async fn a_tab_name_survives_restart_and_folder_change() {
    let root = folder();
    let db = Db::open_in_memory().await.expect("db");
    let before = start(&root, &db).await;
    run(&before, rename(Some("  Work  "))).await;
    run(
        &before,
        Command::Navigate {
            panel: None,
            path: "a".into(),
        },
    )
    .await;
    let shown = tabs(&before).await;
    assert_eq!(shown[0].title, "Work", "kept in another folder");
    assert!(shown[0].renamed);
    run(
        &before,
        Command::SetFileFilter {
            panel: None,
            filter: Some(FileFilter::Programs),
        },
    )
    .await;
    before.save_tabs().await.expect("saved");

    let after = start(&root, &db).await;
    after.restore_tabs().await.expect("restored");
    let shown = tabs(&after).await;
    assert_eq!(shown[0].title, "Work");
    assert_eq!(
        after.panel(PanelId::Left).filter.files,
        FileFilter::Programs,
        "the file filter comes back with its tab"
    );
    run(&after, rename(Some(" "))).await;
    let shown = tabs(&after).await;
    assert_eq!(shown[0].title, "a", "a blank name shows the folder's again");
    assert!(!shown[0].renamed);
}

#[tokio::test]
async fn a_bad_tab_name_is_refused() {
    let root = folder();
    let engine = test_support::headless(root.path()).await;
    for title in ["x".repeat(MAX_TAB_TITLE + 1), "a\u{7}b".to_owned()] {
        let error = engine
            .dispatch(rename(Some(&title)))
            .await
            .expect_err("refused");
        assert_eq!(error.code(), "tab_title_invalid");
    }
    let error = engine
        .dispatch(Command::RenameTab {
            panel: None,
            tab: Some(crate::TabId(999)),
            title: Some("x".into()),
        })
        .await
        .expect_err("no such tab");
    assert_eq!(error.code(), "tab_not_found");
}

#[tokio::test]
async fn close_all_keeps_the_active_and_locked_tabs() {
    let root = folder();
    let engine = test_support::headless(root.path()).await;
    for _ in 0..3 {
        run(&engine, Command::OpenTab(OpenTabRequest::default())).await;
    }
    let rows = tabs(&engine).await;
    assert_eq!(rows.len(), 4);
    run(
        &engine,
        Command::LockTab(LockTabRequest {
            panel: None,
            tab: Some(rows[0].id),
            locked: Some(true),
        }),
    )
    .await;
    let active = rows.iter().find(|tab| tab.active).map(|tab| tab.id);
    let close_all = |include_locked| Command::CloseAllTabs {
        panel: None,
        include_locked,
    };
    let left = test_support::expect_tabs(run(&engine, close_all(false)).await);
    assert_eq!(left.len(), 2);
    assert!(left.iter().any(|tab| tab.locked));
    assert_eq!(left.iter().find(|tab| tab.active).map(|tab| tab.id), active);
    let left = test_support::expect_tabs(run(&engine, close_all(true)).await);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, active.expect("an active tab"));
}
