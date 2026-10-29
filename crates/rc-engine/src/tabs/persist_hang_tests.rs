//! A stored tab on a share that does not answer (RC-080-51): the start waits for it no longer
//! than the drive bar waits for a drive, drops that tab and brings back the others.

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use async_trait::async_trait;
use rc_db::Db;
use rc_vfs::{
    BackendId, Capabilities, Entry, ReadStream, SetAttributes, Vfs, VfsError, WriteMode,
    WriteStream,
};

use super::*;
use crate::test_support;

/// A share whose server is gone without a word: every look waits for ever.
struct Silent;

fn unsupported<T>(path: &Path) -> rc_vfs::Result<T> {
    Err(VfsError::Unsupported {
        path: path.to_path_buf(),
        operation: "anything but a look",
    })
}

#[async_trait]
impl Vfs for Silent {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            rename_across_dirs: false,
            symlinks: false,
            case_sensitive: true,
            unix_mode: false,
            set_mtime: false,
            free_space: false,
        }
    }
    async fn list(&self, path: &Path) -> rc_vfs::Result<Vec<Entry>> {
        unsupported(path)
    }
    async fn stat(&self, _: &Path) -> rc_vfs::Result<Entry> {
        std::future::pending().await
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        unsupported(path)
    }
    async fn open_write(&self, path: &Path, _: WriteMode) -> rc_vfs::Result<WriteStream> {
        unsupported(path)
    }
    async fn mkdir(&self, path: &Path, _: bool) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn rename(&self, from: &Path, _: &Path) -> rc_vfs::Result<()> {
        unsupported(from)
    }
    async fn remove_file(&self, path: &Path) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn remove_dir(&self, path: &Path, _: bool) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn set_mtime(&self, path: &Path, _: SystemTime) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn set_attributes(&self, path: &Path, _: SetAttributes) -> rc_vfs::Result<()> {
        unsupported(path)
    }
}

const SHARE: BackendId = BackendId::Ftp { connection_id: 7 };

fn row(position: i64, active: bool, path: &VfsPath) -> StoredTab {
    StoredTab {
        panel: "left".to_owned(),
        position,
        active,
        locked: false,
        location: serde_json::to_string(path).expect("encodes"),
        sort: String::new(),
        mode: String::new(),
        history: Vec::new(),
    }
}

#[tokio::test]
async fn a_tab_on_a_share_that_does_not_answer_holds_the_start_only_briefly() {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir_all(root.path().join("a")).expect("dir");
    let db = Db::open_in_memory().await.expect("db");
    let local = VfsPath::local(root.path().join("a"));
    let share = VfsPath {
        backend: SHARE,
        path: "/srv/share/deep".into(),
    };
    db.save_tabs(&[row(0, false, &share), row(1, true, &local)])
        .await
        .expect("stored");
    let engine = test_support::headless(root.path()).await;
    assert!(engine.attach_db(db));
    let silent: Arc<dyn Vfs> = Arc::new(Silent);
    engine.inner.remotes.open().insert(SHARE, silent);

    let started = Instant::now();
    let relocated = tokio::time::timeout(
        DETAIL_TIMEOUT + Duration::from_secs(5),
        engine.restore_tabs(),
    )
    .await
    .expect("the start does not hang")
    .expect("restored");
    assert!(started.elapsed() < DETAIL_TIMEOUT + Duration::from_secs(2));
    assert_eq!(relocated, Vec::new());
    let tabs = engine.state().tabs.set(PanelId::Left).tabs().len();
    assert_eq!(tabs, 1, "the silent share's tab is dropped");
    assert_eq!(engine.panel(PanelId::Left).path, local);
}
