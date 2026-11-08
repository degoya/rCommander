//! The folder trees through the engine (RC-100-88): the lazy levels on every kind of backend,
//! what marks them to be read again, the commands and when a panel leaves its tree.

use std::{
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use rc_db::Db;
use rc_vfs::{BackendId, MemoryFs, Vfs, VfsPath};

use crate::test_support::{self, expect_panel};
use crate::{
    Command, DriveKind, Engine, ListMode, Outcome, PanelId, PanelView,
    drives::fake::FakeDrives,
    settings::SEPARATE_TREE_KEY,
    tree::{TreeLevel, tree_chain, tree_root},
};

/// A server in memory, by number.
const SERVER: BackendId = BackendId::Memory(88);

/// A backend in memory that counts its listings.
struct Counting {
    inner: MemoryFs,
    lists: AtomicUsize,
}

#[async_trait::async_trait]
impl rc_vfs::testing::Forward for Counting {
    fn inner(&self) -> &dyn Vfs {
        &self.inner
    }

    async fn list(&self, dir: &Path) -> rc_vfs::Result<Vec<rc_vfs::Entry>> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        self.inner.list(dir).await
    }
}

/// `/a/b/c`, `/a/x` and `/y` on a counting server the engine has open.
async fn server(engine: &Engine) -> Arc<Counting> {
    let fs = MemoryFs::new();
    for dir in ["/a/b/c", "/a/x", "/y"] {
        fs.mkdir(Path::new(dir), true).await.expect("mkdir");
    }
    let counting = Arc::new(Counting {
        inner: fs,
        lists: AtomicUsize::new(0),
    });
    let open: Arc<dyn Vfs> = counting.clone();
    engine.inner.remotes.open().insert(SERVER, open);
    counting
}

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: path.into(),
    }
}

fn names(level: &TreeLevel) -> Vec<String> {
    level
        .children
        .iter()
        .map(|child| child.name.clone())
        .collect()
}

async fn names_of(engine: &Engine, path: &VfsPath) -> Vec<String> {
    names(&engine.tree_children(path).await.expect("a level"))
}

#[tokio::test]
async fn showing_a_folder_reads_only_the_levels_on_its_path_and_each_once() {
    let (_root, engine) = test_support::engine(&[]).await;
    let counting = server(&engine).await;
    let chain = tree_chain(&on_server("/a/b"));
    assert_eq!(chain.first(), Some(&tree_root(&on_server("/a/b"))));
    let mut levels = Vec::new();
    for folder in &chain {
        levels.push(names_of(&engine, folder).await);
    }
    assert_eq!(levels, [vec!["a", "y"], vec!["b", "x"], vec!["c"]]);
    assert_eq!(counting.lists.load(Ordering::SeqCst), chain.len());
    // Asked again, every level is the one kept: no listing more than folders unfolded.
    for folder in &chain {
        engine.tree_children(folder).await.expect("a level");
    }
    assert_eq!(counting.lists.load(Ordering::SeqCst), chain.len());
    engine.tree_reread(&on_server("/a"));
    assert_eq!(names_of(&engine, &on_server("/a")).await, ["b", "x"]);
    assert_eq!(counting.lists.load(Ordering::SeqCst), chain.len() + 1, "F2");
}

#[tokio::test]
async fn files_are_no_folders_of_the_tree_and_hidden_ones_follow_the_panels() {
    let (root, engine) = test_support::engine(&[("sub/", ""), ("file.txt", "x")]).await;
    test_support::hidden_file(&root.path().join(".dot"), "x");
    fs::create_dir(root.path().join(".hidden")).expect("mkdir");
    // A dot folder is hidden on Linux and macOS; on Windows the attribute says so.
    let shown = names_of(&engine, &VfsPath::local(root.path())).await;
    assert!(shown.contains(&"sub".to_owned()), "{shown:?}");
    assert!(!shown.contains(&"file.txt".to_owned()), "{shown:?}");
    engine
        .dispatch(Command::ToggleHidden { show: Some(true) })
        .await
        .expect("shown");
    let all = names_of(&engine, &VfsPath::local(root.path())).await;
    assert!(all.contains(&".hidden".to_owned()), "{all:?}");
    assert!(!all.contains(&".dot".to_owned()), "a file: {all:?}");
}

#[tokio::test]
async fn a_refresh_after_mkdir_rename_or_delete_reads_the_folder_again() {
    let (root, engine) = test_support::engine(&[("old/", ""), ("gone/", "")]).await;
    let here = VfsPath::local(root.path());
    assert_eq!(names_of(&engine, &here).await, ["gone", "old"]);
    fs::create_dir(root.path().join("new")).expect("mkdir");
    fs::rename(root.path().join("old"), root.path().join("renamed")).expect("rename");
    fs::remove_dir(root.path().join("gone")).expect("delete");
    assert_eq!(
        names_of(&engine, &here).await,
        ["gone", "old"],
        "kept until the folder is read again"
    );
    engine
        .dispatch(Command::Refresh { panel: None })
        .await
        .expect("refreshed");
    assert_eq!(names_of(&engine, &here).await, ["new", "renamed"]);
    // A change the window reports by its path marks the folder holding it.
    fs::create_dir(root.path().join("made")).expect("mkdir");
    engine.refresh_showing(&[here.join("made")]).await;
    assert_eq!(names_of(&engine, &here).await, ["made", "new", "renamed"]);
}

#[tokio::test]
async fn an_archive_lists_its_folders_from_its_contents() {
    let (root, engine) = crate::archives::tests::engine_beside("tree.zip").await;
    let file = VfsPath::local(root.path().join("tree.zip"));
    let archive = engine
        .archive_root(&file)
        .await
        .expect("readable")
        .expect("an archive");
    assert_eq!(tree_root(&archive.join("docs")), archive);
    let folders = names_of(&engine, &archive).await;
    assert!(folders.contains(&"docs".to_owned()), "{folders:?}");
    assert!(!folders.contains(&"hello.txt".to_owned()), "{folders:?}");
}

#[tokio::test]
async fn an_sftp_server_unfolds_one_level_at_a_time_and_a_failure_is_the_node_s() {
    let harness = crate::remote::sftp::tests::harness().await;
    std::fs::create_dir_all(harness.served.path().join("home/tester/docs/deeper")).expect("mkdir");
    let engine = &harness.engine;
    engine
        .dispatch(Command::ChangeDrive {
            panel: Some(PanelId::Left),
            drive: format!("server:{}", harness.id),
        })
        .await
        .expect("the server opens");
    let home = engine.panel_path(PanelId::Left);
    assert!(matches!(home.backend, BackendId::Sftp { .. }));
    assert_eq!(names_of(engine, &home).await, ["docs"]);
    assert_eq!(names_of(engine, &home.join("docs")).await, ["deeper"]);
    let missing = engine
        .tree_children(&home.join("missing"))
        .await
        .expect_err("no such folder");
    assert!(!missing.code().is_empty());
}

#[tokio::test]
async fn ctrl_f8_turns_the_panel_into_its_tree_and_back() {
    let (_root, engine) = test_support::engine(&[("sub/", "")]).await;
    let show = |show| Command::ShowTree {
        panel: Some(PanelId::Left),
        show,
    };
    let tree = expect_panel(engine.dispatch(show(None)).await.expect("shown"));
    assert_eq!(tree.view, PanelView::Tree);
    let again = expect_panel(engine.dispatch(show(Some(true))).await.expect("shown"));
    assert_eq!(again.view, PanelView::Tree, "already its tree");
    let files = expect_panel(engine.dispatch(show(None)).await.expect("shown"));
    assert_eq!(files.view, PanelView::Files);
    assert_eq!(engine.panel(PanelId::Right).view, PanelView::Files);
}

#[tokio::test]
async fn ctrl_f1_and_ctrl_f2_end_the_tree() {
    let (_root, engine) = test_support::engine(&[]).await;
    for mode in [ListMode::Brief, ListMode::Full] {
        engine
            .dispatch(Command::ShowTree {
                panel: None,
                show: Some(true),
            })
            .await
            .expect("shown");
        let after = expect_panel(
            engine
                .dispatch(Command::SetListMode { panel: None, mode })
                .await
                .expect("set"),
        );
        assert_eq!((after.view, after.mode), (PanelView::Files, mode));
    }
}

#[tokio::test]
async fn a_drive_change_ends_the_tree() {
    let root = test_support::tree(&[("a/sub/", ""), ("b/", "")]);
    let fake = FakeDrives::default()
        .with("A:", DriveKind::Fixed, root.path().join("a"))
        .with("B:", DriveKind::Fixed, root.path().join("b"));
    let start = VfsPath::local(root.path().join("a"));
    let engine = Engine::open_with(
        start.clone(),
        start,
        Arc::new(fake),
        Duration::from_millis(100),
    )
    .await
    .expect("engine");
    engine
        .dispatch(Command::ShowTree {
            panel: Some(PanelId::Left),
            show: Some(true),
        })
        .await
        .expect("shown");
    let changed = engine
        .dispatch(Command::ChangeDrive {
            panel: Some(PanelId::Left),
            drive: "B:".into(),
        })
        .await
        .expect("changed");
    let Outcome::Panel(snapshot) = changed else {
        panic!("a panel");
    };
    assert_eq!(snapshot.view, PanelView::Files);
    assert_eq!(snapshot.path, VfsPath::local(root.path().join("b")));
}

#[tokio::test]
async fn ctrl_shift_f8_turns_the_separate_tree_as_total_commander_does() {
    let (_root, engine) = test_support::engine(&[]).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let mut seen = Vec::new();
    for _ in 0..3 {
        let Ok(Outcome::Settings { settings }) =
            engine.dispatch(Command::CycleSeparateTree {}).await
        else {
            panic!("the setting");
        };
        assert_eq!(settings[0].key, SEPARATE_TREE_KEY);
        seen.push(settings[0].value.clone());
    }
    assert_eq!(seen, ["one", "two", "off"]);
}
