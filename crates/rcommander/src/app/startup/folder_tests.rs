use std::time::Instant;

use super::*;

/// A start folder on a file system that never answers - a share whose server is gone - holds
/// the start no longer than the limit, and is reported.
#[tokio::test]
async fn a_start_folder_that_does_not_answer_holds_the_start_only_for_the_limit() {
    let limit = Duration::from_millis(200);
    let folder = Path::new("/srv/share/gone");
    let started = Instant::now();
    let looked = tokio::time::timeout(
        Duration::from_secs(10),
        look(folder, std::future::pending(), limit),
    )
    .await
    .expect("the start does not hang");
    assert_eq!(looked, Looked::Silent(folder.to_path_buf()));
    assert!(
        started.elapsed() < limit + Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_start_folder_is_taken_only_when_it_is_a_folder() {
    let root = tempfile::tempdir().expect("temp dir");
    let file = root.path().join("file.txt");
    std::fs::write(&file, b"x").expect("write");
    let limit = Duration::from_secs(10);
    assert_eq!(
        look_locally(Some(root.path().to_path_buf()), limit).await,
        Looked::Folder(root.path().to_path_buf())
    );
    assert_eq!(look_locally(Some(file), limit).await, Looked::Passed);
    assert_eq!(
        look_locally(Some(root.path().join("gone")), limit).await,
        Looked::Passed
    );
    assert_eq!(look_locally(None, limit).await, Looked::Passed);
}

#[tokio::test]
async fn without_a_database_both_panels_start_at_home() {
    let home = Path::new("/home/someone");
    let folders = start_folders(None, home).await;
    assert_eq!(folders.left, home);
    assert_eq!(folders.right, home);
    assert_eq!(folders.silent, None);
}

/// A start folder and stored tabs on the same dead share: looked at side by side, as
/// `place_panels` joins them, they hold the start for one limit, not two. The tabs' own look
/// ends at the same limit (rc-engine `tabs/persist_hang_tests.rs`); a look that never answers
/// stands in for it here.
#[tokio::test]
async fn a_dead_start_folder_and_dead_tabs_hold_the_start_once() {
    let dead = Path::new("/srv/share/gone");
    let started = Instant::now();
    let tabs = tokio::time::timeout(DETAIL_TIMEOUT, std::future::pending::<()>());
    let (looked, tabs) = tokio::join!(look(dead, std::future::pending(), DETAIL_TIMEOUT), tabs);
    assert_eq!(looked, Looked::Silent(dead.to_path_buf()));
    assert!(tabs.is_err(), "the tabs' look ran into its limit");
    assert!(
        started.elapsed() < DETAIL_TIMEOUT.mul_f64(1.5),
        "{:?}",
        started.elapsed()
    );
}

/// A panel whose stored tab came back shows it; the other moves to its start folder.
#[tokio::test]
async fn restored_tabs_win_and_a_panel_without_one_takes_its_start_folder() {
    if std::env::var_os("RC_START_DIR").is_some() {
        return;
    }
    let root = tempfile::tempdir().expect("temp dir");
    for name in ["home", "left", "right", "tab"] {
        std::fs::create_dir_all(root.path().join(name)).expect("dir");
    }
    let home = root.path().join("home");
    let db = Db::open_in_memory().await.expect("db");
    db.set(LEFT_START_KEY, &root.path().join("left"))
        .await
        .expect("left");
    db.set(RIGHT_START_KEY, &root.path().join("right"))
        .await
        .expect("right");
    let tab = rc_engine::VfsPath::local(root.path().join("tab"));
    db.save_tabs(&[rc_db::StoredTab {
        panel: "left".to_owned(),
        position: 0,
        active: true,
        locked: false,
        location: serde_json::to_string(&tab).expect("encodes"),
        sort: String::new(),
        mode: String::new(),
        title: None,
        file_filter: "null".to_owned(),
        history: Vec::new(),
    }])
    .await
    .expect("tabs");
    let engine = Engine::open(
        rc_engine::VfsPath::local(home.clone()),
        rc_engine::VfsPath::local(home.clone()),
    )
    .await
    .expect("engine");
    assert!(engine.attach_db(db.clone()));

    let placed = place_panels(&engine, Some(&db), &home, true).await;
    assert_eq!(placed.silent, None);
    assert_eq!(engine.panel(PanelId::Left).path, tab);
    assert_eq!(
        engine.panel(PanelId::Right).path,
        rc_engine::VfsPath::local(root.path().join("right"))
    );
}
