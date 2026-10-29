use rc_db::Db;
use tempfile::TempDir;

use super::*;
use crate::test_support;
use crate::{
    Command, LockTabRequest, MoveTabRequest, Outcome, PanelRequest, TabView,
    tabs::requests::{OpenTabRequest, SelectTabRequest},
};

/// A folder with `a`, `b` and `c/deep`.
fn folder() -> TempDir {
    let root = tempfile::tempdir().expect("temp dir");
    for dir in ["a", "b", "c/deep"] {
        std::fs::create_dir_all(root.path().join(dir)).expect("dir");
    }
    root
}

/// An engine on `root`, as a fresh start, with `db`.
async fn start(root: &TempDir, db: &Db) -> Engine {
    let engine = test_support::headless(root.path()).await;
    assert!(engine.attach_db(db.clone()));
    engine
}

async fn run(engine: &Engine, command: Command) -> Outcome {
    engine.dispatch(command).await.expect("command")
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

async fn tabs(engine: &Engine, panel: PanelId) -> Vec<TabView> {
    let listed = run(
        engine,
        Command::ListTabs(PanelRequest { panel: Some(panel) }),
    )
    .await;
    test_support::expect_tabs(listed)
}

/// The acceptance check of RC-010-03: tabs, a locked tab, the shown tab and each tab's history
/// survive a restart (against a temporary database).
#[tokio::test]
async fn tabs_and_locked_tabs_survive_a_restart() {
    let root = folder();
    let db = Db::open_in_memory().await.expect("db");
    let before = start(&root, &db).await;
    go(&before, PanelId::Left, "a").await;
    run(&before, Command::OpenTab(OpenTabRequest::default())).await;
    go(&before, PanelId::Left, "../b").await;
    run(
        &before,
        Command::LockTab(LockTabRequest {
            panel: Some(PanelId::Left),
            locked: Some(true),
            tab: None,
        }),
    )
    .await;
    go(&before, PanelId::Right, "c").await;
    let saved_left = tabs(&before, PanelId::Left).await;
    before.save_tabs().await.expect("save");

    let after = start(&root, &db).await;
    assert_eq!(after.restore_tabs().await.expect("restore"), Vec::new());
    let left = tabs(&after, PanelId::Left).await;
    let strip = |tabs: &[TabView]| -> Vec<(String, bool, bool)> {
        tabs.iter()
            .map(|tab| (tab.title.clone(), tab.locked, tab.active))
            .collect()
    };
    assert_eq!(strip(&left), strip(&saved_left));
    assert_eq!(
        strip(&left),
        [("a".to_owned(), false, false), ("b".to_owned(), true, true)]
    );
    assert_eq!(
        after.panel(PanelId::Left).path,
        VfsPath::local(root.path().join("b"))
    );
    assert_eq!(
        after.panel(PanelId::Right).path,
        VfsPath::local(root.path().join("c"))
    );
    // The first tab's history came back: root, then `a`.
    run(
        &after,
        Command::SelectTab(SelectTabRequest {
            panel: Some(PanelId::Left),
            tab: left[0].id,
        }),
    )
    .await;
    run(
        &after,
        Command::HistoryBack(PanelRequest {
            panel: Some(PanelId::Left),
        }),
    )
    .await;
    assert_eq!(after.panel(PanelId::Left).path, VfsPath::local(root.path()));
}

/// A tab moved in its row (RC-010-24) keeps its new place across a restart.
#[tokio::test]
async fn a_moved_tab_keeps_its_place_across_a_restart() {
    let root = folder();
    let db = Db::open_in_memory().await.expect("db");
    let before = start(&root, &db).await;
    go(&before, PanelId::Left, "a").await;
    run(&before, Command::OpenTab(OpenTabRequest::default())).await;
    go(&before, PanelId::Left, "../b").await;
    let first = tabs(&before, PanelId::Left).await[0].id;
    run(
        &before,
        Command::MoveTab(MoveTabRequest {
            panel: Some(PanelId::Left),
            tab: first,
            to: 1,
        }),
    )
    .await;
    before.save_tabs().await.expect("save");

    let after = start(&root, &db).await;
    after.restore_tabs().await.expect("restore");
    let left: Vec<(String, bool)> = tabs(&after, PanelId::Left)
        .await
        .into_iter()
        .map(|tab| (tab.title, tab.active))
        .collect();
    assert_eq!(left, [("b".to_owned(), true), ("a".to_owned(), false)]);
}

/// The acceptance check of RC-010-03: a stored tab on a deleted folder opens the nearest folder
/// above it that is still there, and says so.
#[tokio::test]
async fn a_tab_on_a_deleted_folder_opens_the_nearest_parent() {
    let root = folder();
    let db = Db::open_in_memory().await.expect("db");
    let before = start(&root, &db).await;
    go(&before, PanelId::Right, "c/deep").await;
    before.save_tabs().await.expect("save");
    std::fs::remove_dir(root.path().join("c").join("deep")).expect("remove");

    let after = start(&root, &db).await;
    let relocated = after.restore_tabs().await.expect("restore");
    assert_eq!(
        relocated,
        [Relocated {
            panel: PanelId::Right,
            stored: VfsPath::local(root.path().join("c").join("deep")),
            shown: VfsPath::local(root.path().join("c")),
        }]
    );
    assert_eq!(
        after.panel(PanelId::Right).path,
        VfsPath::local(root.path().join("c"))
    );
}

#[tokio::test]
async fn nothing_stored_leaves_the_panels_as_they_started() {
    let root = folder();
    let db = Db::open_in_memory().await.expect("db");
    let engine = start(&root, &db).await;
    assert_eq!(engine.restore_tabs().await.expect("restore"), Vec::new());
    assert_eq!(tabs(&engine, PanelId::Left).await.len(), 1);
    assert_eq!(
        engine.panel(PanelId::Left).path,
        VfsPath::local(root.path())
    );
}

#[tokio::test]
async fn without_a_database_nothing_is_stored() {
    let root = folder();
    let engine = test_support::headless(root.path()).await;
    let error = engine.save_tabs().await.expect_err("no database");
    assert_eq!(error.code(), "database_unavailable");
}

#[test]
fn a_row_that_does_not_read_is_no_tab() {
    let row = StoredTab {
        panel: "left".into(),
        position: 0,
        active: true,
        locked: false,
        location: "not json".into(),
        sort: String::new(),
        mode: String::new(),
        history: Vec::new(),
    };
    assert_eq!(tab_from_row(&row), None);
    let path = VfsPath::local("/x");
    let readable = StoredTab {
        location: serde_json::to_string(&path).expect("json"),
        ..row
    };
    let tab = tab_from_row(&readable).expect("a tab");
    assert_eq!(tab.path, path);
    assert_eq!(
        tab.sort,
        SortOrder::default(),
        "an unreadable sort is the default"
    );
}

/// `folder()` with a copy of the archive fixture `tree.zip`.
fn folder_with_archive() -> TempDir {
    let root = folder();
    let fixture = crate::archives::tests::fixture("tree.zip");
    std::fs::copy(fixture, root.path().join("tree.zip")).expect("copy the fixture");
    root
}

/// The acceptance check of RC-030-02: a tab inside an archive survives a restart, with the
/// archive's details for its listing.
#[tokio::test]
async fn a_tab_inside_an_archive_survives_a_restart() {
    let root = folder_with_archive();
    let inside = root.path().join("tree.zip").join("docs");
    let db = Db::open_in_memory().await.expect("db");
    let before = start(&root, &db).await;
    go(&before, PanelId::Left, &inside.to_string_lossy()).await;
    let stored = before.panel(PanelId::Left).path;
    assert!(matches!(stored.backend, rc_vfs::BackendId::Archive { .. }));
    before.save_tabs().await.expect("save");

    let after = start(&root, &db).await;
    assert_eq!(after.restore_tabs().await.expect("restore"), Vec::new());
    let shown = after.panel(PanelId::Left);
    assert_eq!(shown.path, stored);
    assert_eq!(crate::shown_path(&shown.path), inside);
    assert!(shown.index_of("readme.md").is_some());
    assert!(shown.archive.is_some(), "the archive's details");
}

/// RC-030-02: a stored tab inside an archive whose file is gone opens the folder that held the
/// archive, and says so - no error.
#[tokio::test]
async fn a_tab_inside_a_deleted_archive_opens_the_archive_folder() {
    let root = folder_with_archive();
    let inside = root.path().join("tree.zip").join("docs");
    let db = Db::open_in_memory().await.expect("db");
    let before = start(&root, &db).await;
    go(&before, PanelId::Right, &inside.to_string_lossy()).await;
    let stored = before.panel(PanelId::Right).path;
    before.save_tabs().await.expect("save");
    std::fs::remove_file(root.path().join("tree.zip")).expect("remove");

    let after = start(&root, &db).await;
    let relocated = after.restore_tabs().await.expect("restore");
    assert_eq!(
        relocated,
        [Relocated {
            panel: PanelId::Right,
            stored,
            shown: VfsPath::local(root.path()),
        }]
    );
    let shown = after.panel(PanelId::Right);
    assert_eq!(shown.path, VfsPath::local(root.path()));
    assert!(shown.archive.is_none());
}

/// A tab inside an archive that went while the app ran: selecting it shows the folder that
/// held the archive.
#[tokio::test]
async fn selecting_a_tab_whose_archive_is_gone_shows_the_archive_folder() {
    let root = folder_with_archive();
    let inside = root.path().join("tree.zip").join("docs");
    let db = Db::open_in_memory().await.expect("db");
    let engine = start(&root, &db).await;
    go(&engine, PanelId::Left, &inside.to_string_lossy()).await;
    run(&engine, Command::OpenTab(OpenTabRequest::default())).await;
    assert!(
        engine.panel(PanelId::Left).archive.is_some(),
        "a new tab inside an archive has the archive's details"
    );
    // Out of the archive by its parents: on Linux an absolute path typed inside an archive
    // stays inside it (RC-030-03).
    go(&engine, PanelId::Left, "..").await;
    go(&engine, PanelId::Left, "..").await;
    go(&engine, PanelId::Left, "a").await;
    assert_eq!(
        engine.panel(PanelId::Left).path,
        VfsPath::local(root.path().join("a"))
    );
    std::fs::remove_file(root.path().join("tree.zip")).expect("remove");
    let first = tabs(&engine, PanelId::Left).await[0].id;
    run(
        &engine,
        Command::SelectTab(SelectTabRequest {
            panel: Some(PanelId::Left),
            tab: first,
        }),
    )
    .await;
    assert_eq!(
        engine.panel(PanelId::Left).path,
        VfsPath::local(root.path())
    );
}
