use std::{
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use rc_db::Db;
use rc_vfs::{BackendId, VfsPath};
use tempfile::TempDir;

use crate::test_support::{self, expect_drives};
use crate::{
    AuthMethod, Command, ConnectionDraft, ConnectionKind, Drive, DriveGroup, DriveKind, DriveState,
    Engine, EngineError, EngineEvent, Outcome, PanelId, drives::fake::FakeDrives,
    settings::FOLLOW_OTHER_PANEL_KEY,
};

/// `a/` (with `sub/deep/`), `b/` and `d/`: the roots of the fake drives `A:`, `B:` and `D:`.
fn tree() -> TempDir {
    test_support::tree(&[("a/sub/deep/", ""), ("b/inner/", ""), ("d/", "")])
}

fn fake(root: &Path) -> FakeDrives {
    FakeDrives::default()
        .with("A:", DriveKind::Fixed, root.join("a"))
        .with("B:", DriveKind::Removable, root.join("b"))
        .with("D:", DriveKind::Optical, root.join("d"))
        .failing("D:")
}

async fn engine(fake: FakeDrives, start: &Path) -> Engine {
    Engine::open_with(
        VfsPath::local(start),
        VfsPath::local(start),
        Arc::new(fake),
        Duration::from_millis(100),
    )
    .await
    .expect("engine")
}

async fn change(engine: &Engine, panel: PanelId, drive: &str) -> Result<Outcome, EngineError> {
    engine
        .dispatch(Command::ChangeDrive {
            panel: Some(panel),
            drive: drive.into(),
        })
        .await
}

fn path(engine: &Engine, panel: PanelId) -> std::path::PathBuf {
    engine.panel(panel).path.path
}

async fn navigate(engine: &Engine, panel: PanelId, to: &Path) {
    engine
        .dispatch(Command::Navigate {
            panel: Some(panel),
            path: to.to_string_lossy().into_owned(),
        })
        .await
        .expect("navigates");
}

/// `drives.follow_other_panel` switched off, in a database of its own: a drive change goes by
/// this panel's memory only, as before the setting.
async fn without_following(engine: &Engine) {
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    engine
        .dispatch(Command::SetSetting {
            key: FOLLOW_OTHER_PANEL_KEY.into(),
            value: serde_json::json!(false),
        })
        .await
        .expect("stored");
}

#[tokio::test]
async fn the_list_has_the_drives_with_their_kind_and_space() {
    let root = tree();
    let engine = engine(fake(root.path()), &root.path().join("a")).await;
    let drives = expect_drives(
        engine
            .dispatch(Command::ListDrives { refresh: false })
            .await
            .expect("lists"),
    );
    let summary: Vec<(&str, DriveGroup, DriveKind, DriveState)> = drives
        .iter()
        .map(|drive| (drive.id.as_str(), drive.group, drive.kind, drive.state))
        .collect();
    assert_eq!(
        summary,
        [
            (
                "A:",
                DriveGroup::Drives,
                DriveKind::Fixed,
                DriveState::Ready
            ),
            (
                "B:",
                DriveGroup::Drives,
                DriveKind::Removable,
                DriveState::Ready
            ),
            (
                "D:",
                DriveGroup::Drives,
                DriveKind::Optical,
                DriveState::NotReady
            ),
        ]
    );
    assert_eq!(drives[0].root, Some(VfsPath::local(root.path().join("a"))));
    assert!(drives[0].space.is_some());
}

#[tokio::test]
async fn a_refresh_is_announced_a_plain_list_is_not() {
    let root = tree();
    let engine = engine(fake(root.path()), &root.path().join("a")).await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("lists");
    assert!(events.try_recv().is_err(), "a list announces nothing");
    engine
        .dispatch(Command::ListDrives { refresh: true })
        .await
        .expect("refreshes");
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::Drives));
}

#[tokio::test]
async fn a_drive_change_returns_to_the_last_directory_of_that_drive_per_panel() {
    let root = tree();
    let engine = engine(fake(root.path()), &root.path().join("a")).await;
    // The right panel stays on `A:`: followed, it would decide where the left one lands.
    without_following(&engine).await;
    let a = root.path().join("a");
    let b = root.path().join("b");
    // The drive bar was read once, as the window does on start.
    engine
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("lists");
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: a.join("sub").join("deep").to_string_lossy().into_owned(),
        })
        .await
        .expect("navigates");

    change(&engine, PanelId::Left, "B:").await.expect("changes");
    assert_eq!(
        path(&engine, PanelId::Left),
        b,
        "a new drive opens at its root"
    );
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: "inner".into(),
        })
        .await
        .expect("navigates");

    change(&engine, PanelId::Left, "a").await.expect("changes");
    assert_eq!(path(&engine, PanelId::Left), a.join("sub").join("deep"));
    change(&engine, PanelId::Left, "b:\\")
        .await
        .expect("changes");
    assert_eq!(path(&engine, PanelId::Left), b.join("inner"));

    // The right panel keeps its own memory.
    change(&engine, PanelId::Right, "A:")
        .await
        .expect("changes");
    assert_eq!(path(&engine, PanelId::Right), a);
}

#[tokio::test]
async fn a_remembered_directory_that_is_gone_falls_back_to_the_root() {
    let root = tree();
    let engine = engine(fake(root.path()), &root.path().join("a")).await;
    without_following(&engine).await;
    let a = root.path().join("a");
    engine
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("lists");
    engine
        .dispatch(Command::Navigate {
            panel: None,
            path: a.join("sub").to_string_lossy().into_owned(),
        })
        .await
        .expect("navigates");
    change(&engine, PanelId::Left, "B:").await.expect("changes");
    fs::remove_dir_all(a.join("sub")).expect("rm");
    change(&engine, PanelId::Left, "A:").await.expect("changes");
    assert_eq!(path(&engine, PanelId::Left), a);
}

/// Total Commander (owner 2026-10-19): a drive the other panel shows opens at its folder; the
/// panel's own drive is not followed, it stays where it is.
#[tokio::test]
async fn a_drive_the_other_panel_shows_opens_at_the_other_panel_s_folder() {
    let root = tree();
    let engine = engine(fake(root.path()), &root.path().join("a")).await;
    let b = root.path().join("b");
    engine
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("lists");
    navigate(&engine, PanelId::Right, &b.join("inner")).await;

    change(&engine, PanelId::Left, "b:").await.expect("changes");
    assert_eq!(path(&engine, PanelId::Left), b.join("inner"));

    navigate(&engine, PanelId::Left, &b).await;
    change(&engine, PanelId::Left, "B:").await.expect("changes");
    assert_eq!(
        path(&engine, PanelId::Left),
        b,
        "the panel's own drive is not followed"
    );
}

/// The other panel on another drive: this panel's memory, else the root, as before.
#[tokio::test]
async fn the_other_panel_on_another_drive_leaves_the_memory_or_the_root() {
    let root = tree();
    fs::create_dir(root.path().join("c")).expect("mkdir");
    let drives = fake(root.path()).with("C:", DriveKind::Fixed, root.path().join("c"));
    let engine = engine(drives, &root.path().join("a")).await;
    let (a, b) = (root.path().join("a"), root.path().join("b"));
    engine
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("lists");
    navigate(&engine, PanelId::Right, &root.path().join("c")).await;
    navigate(&engine, PanelId::Left, &a.join("sub").join("deep")).await;

    change(&engine, PanelId::Left, "B:").await.expect("changes");
    assert_eq!(path(&engine, PanelId::Left), b, "a new drive at its root");
    change(&engine, PanelId::Left, "A:").await.expect("changes");
    assert_eq!(path(&engine, PanelId::Left), a.join("sub").join("deep"));
}

/// The other panel's folder is gone: the panel's memory of the drive, then its root.
#[tokio::test]
async fn a_followed_folder_that_is_gone_falls_back_to_the_memory_then_the_root() {
    let root = tree();
    let engine = engine(fake(root.path()), &root.path().join("a")).await;
    let b = root.path().join("b");
    let gone = b.join("gone");
    fs::create_dir(&gone).expect("mkdir");
    engine
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("lists");
    navigate(&engine, PanelId::Left, &b.join("inner")).await;
    navigate(&engine, PanelId::Left, &root.path().join("a")).await;
    navigate(&engine, PanelId::Right, &gone).await;
    fs::remove_dir(&gone).expect("rm");

    change(&engine, PanelId::Left, "B:").await.expect("changes");
    assert_eq!(path(&engine, PanelId::Left), b.join("inner"));

    navigate(&engine, PanelId::Left, &root.path().join("a")).await;
    fs::remove_dir(b.join("inner")).expect("rm");
    change(&engine, PanelId::Left, "B:").await.expect("changes");
    assert_eq!(path(&engine, PanelId::Left), b);
}

/// Off, the other panel is not followed: the panel's memory, else the root.
#[tokio::test]
async fn with_following_off_the_other_panel_s_folder_is_not_taken() {
    let root = tree();
    let engine = engine(fake(root.path()), &root.path().join("a")).await;
    without_following(&engine).await;
    let b = root.path().join("b");
    engine
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("lists");
    navigate(&engine, PanelId::Right, &b.join("inner")).await;
    change(&engine, PanelId::Left, "B:").await.expect("changes");
    assert_eq!(path(&engine, PanelId::Left), b);
}

#[tokio::test]
async fn a_drive_that_is_not_ready_leaves_the_panel_where_it_is() {
    let root = tree();
    let start = root.path().join("a");
    let engine = engine(fake(root.path()), &start).await;
    let refused = change(&engine, PanelId::Left, "D:").await;
    assert_eq!(refused.expect_err("not ready").code(), "drive_not_ready");
    assert_eq!(path(&engine, PanelId::Left), start);
}

/// The acceptance's simulated time limit, through the command: a hanging drive answers
/// `drive_not_ready` after the limit instead of holding the panel.
#[tokio::test]
async fn a_hanging_drive_is_refused_within_the_time_limit() {
    let root = tree();
    let start = root.path().join("a");
    let slow = fake(root.path()).slow("B:", Duration::from_millis(800));
    let engine = engine(slow, &start).await;
    let started = Instant::now();
    let refused = change(&engine, PanelId::Left, "B:").await;
    assert!(
        started.elapsed() < Duration::from_millis(600),
        "waited {:?}",
        started.elapsed()
    );
    assert_eq!(refused.expect_err("not ready").code(), "drive_not_ready");
    assert_eq!(path(&engine, PanelId::Left), start);
    // Let the hanging question end before the runtime does.
    tokio::time::sleep(Duration::from_millis(900)).await;
}

#[tokio::test]
async fn an_unknown_drive_is_refused_an_ssh_server_opens_over_sftp_an_ftp_one_over_ftp() {
    let root = tree();
    let engine = engine(fake(root.path()), &root.path().join("a")).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db));
    let ssh = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("web", "example.org"),
        })
        .await
        .expect("connection");
    let Outcome::Connection { connection: ssh } = ssh else {
        panic!("a connection");
    };
    engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft {
                kind: ConnectionKind::Ftp,
                auth_method: AuthMethod::Anonymous,
                ..ConnectionDraft::new("files", "example.org")
            },
        })
        .await
        .expect("connection");
    let drives = expect_drives(
        engine
            .dispatch(Command::ListDrives { refresh: false })
            .await
            .expect("lists"),
    );
    let servers: Vec<&Drive> = drives
        .iter()
        .filter(|drive| drive.group == DriveGroup::Servers)
        .collect();
    let web = servers
        .iter()
        .find(|drive| drive.label == "web")
        .expect("web");
    // An SSH server opens over SFTP: its id is its drive, `change_drive` asks for its home.
    assert_eq!(
        web.root,
        Some(VfsPath {
            backend: BackendId::Sftp {
                connection_id: ssh.id.0,
            },
            path: "/".into(),
        })
    );
    assert_eq!(web.id, format!("server:{}", ssh.id));
    let server = servers
        .iter()
        .find(|drive| drive.label == "files")
        .expect("ftp");
    // An FTP server opens over FTP (RC-070-03; `remote/ftp_panel_tests.rs` opens one).
    assert_eq!(
        server.root.as_ref().map(|root| &root.backend),
        Some(&BackendId::Ftp {
            connection_id: server
                .id
                .trim_start_matches("server:")
                .parse()
                .expect("an id"),
        })
    );
    let unknown = change(&engine, PanelId::Left, "Q:").await;
    assert_eq!(unknown.expect_err("unknown").code(), "drive_not_found");
}

#[tokio::test]
async fn the_drive_menu_is_announced_for_its_panel() {
    let root = tree();
    let engine = engine(fake(root.path()), &root.path().join("a")).await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::DriveMenu {
            panel: Some(PanelId::Right),
        })
        .await
        .expect("announces");
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::DriveMenu {
            panel: PanelId::Right
        })
    );
}

#[tokio::test]
async fn a_server_in_a_folder_carries_its_folders_from_the_root_down() {
    let root = tree();
    let engine = engine(fake(root.path()), &root.path().join("a")).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let folder = |parent, name: &str| Command::CreateFolder {
        parent_id: parent,
        name: name.to_owned(),
    };
    let Ok(Outcome::Folder { folder: work }) = engine.dispatch(folder(None, "Work")).await else {
        panic!("a folder");
    };
    let Ok(Outcome::Folder { folder: staging }) =
        engine.dispatch(folder(Some(work.id), "Staging")).await
    else {
        panic!("a folder");
    };
    for (folder_id, name) in [(None, "top"), (Some(staging.id), "deep")] {
        engine
            .dispatch(Command::CreateConnection {
                folder_id,
                connection: ConnectionDraft::new(name, "example.org"),
            })
            .await
            .expect("connection");
    }
    let drives = expect_drives(
        engine
            .dispatch(Command::ListDrives { refresh: false })
            .await
            .expect("lists"),
    );
    let servers: Vec<(&str, Vec<String>)> = drives
        .iter()
        .filter(|drive| drive.group == DriveGroup::Servers)
        .map(|drive| (drive.label.as_str(), drive.folder.clone()))
        .collect();
    assert_eq!(
        servers,
        [
            ("top", Vec::new()),
            ("deep", vec!["Work".to_owned(), "Staging".to_owned()]),
        ]
    );
}
