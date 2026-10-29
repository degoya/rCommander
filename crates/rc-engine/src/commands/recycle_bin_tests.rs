use std::{path::Path, sync::Arc, time::Duration};

use async_trait::async_trait;
use rc_ops::{Backend, DeleteMode, ErrorPolicy, OpsError, Trash};
use rc_vfs::{BackendId, LocalFs, VfsPath};
use serde_json::json;

use crate::test_support;
use crate::{
    Command, Engine, EngineEvent, FileDialog, Outcome, settings::PERMANENT_WITHOUT_TRASH_KEY,
};

/// A recycle bin that moves an entry into `bin/` beside it.
struct Bin;

#[async_trait]
impl Trash for Bin {
    async fn trash(&self, path: &Path) -> Result<(), OpsError> {
        let bin = path.parent().unwrap_or(path).join("bin");
        let name = path.file_name().unwrap_or_default();
        tokio::fs::create_dir_all(&bin)
            .await
            .map_err(|error| failed(path, &error))?;
        tokio::fs::rename(path, bin.join(name))
            .await
            .map_err(|error| failed(path, &error))
    }
}

fn failed(path: &Path, error: &std::io::Error) -> OpsError {
    OpsError::TrashFailed {
        path: path.to_path_buf(),
        reason: error.to_string(),
    }
}

/// `root/` with `a.txt`, a database, and the local backend with or without a recycle bin.
async fn engine(bin: bool) -> (tempfile::TempDir, Engine) {
    let (root, engine) = test_support::engine_with_db(&[("a.txt", "a")]).await;
    let trash: Option<Arc<dyn Trash>> = bin.then(|| Arc::new(Bin) as Arc<dyn Trash>);
    engine.ops().register(
        BackendId::Local,
        Backend::new(Arc::new(LocalFs::new())).with_trash(trash),
    );
    (root, engine)
}

async fn switch_on(engine: &Engine) {
    engine
        .dispatch(Command::SetSetting {
            key: PERMANENT_WITHOUT_TRASH_KEY.into(),
            value: json!(true),
        })
        .await
        .expect("an agent may switch it");
}

/// The dialog `ops.dialog` announced for `asked`.
async fn dialog(engine: &Engine, asked: FileDialog) -> FileDialog {
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::FileDialog {
            panel: None,
            dialog: asked,
        })
        .await
        .expect("dialog");
    loop {
        if let EngineEvent::FileDialog { dialog, .. } = events.try_recv().expect("an event") {
            return dialog;
        }
    }
}

#[tokio::test]
async fn off_f8_keeps_its_dialog_where_there_is_no_bin() {
    let (_root, engine) = engine(false).await;
    assert_eq!(
        dialog(&engine, FileDialog::Delete).await,
        FileDialog::Delete
    );
}

#[tokio::test]
async fn on_f8_opens_the_permanent_delete_where_there_is_no_bin() {
    let (_root, engine) = engine(false).await;
    switch_on(&engine).await;
    assert_eq!(
        dialog(&engine, FileDialog::Delete).await,
        FileDialog::DeletePermanent
    );
    for other in [
        FileDialog::DeletePermanent,
        FileDialog::Copy,
        FileDialog::Mkdir,
    ] {
        assert_eq!(dialog(&engine, other).await, other, "{other:?}");
    }
}

/// Where a bin is, F8 stays F8 and its delete goes into the bin.
#[tokio::test]
async fn on_a_folder_with_a_bin_still_deletes_into_the_bin() {
    let (root, engine) = engine(true).await;
    switch_on(&engine).await;
    assert_eq!(
        dialog(&engine, FileDialog::Delete).await,
        FileDialog::Delete
    );
    let outcome = engine
        .dispatch(Command::OpsDelete(crate::DeleteRequest {
            panel: None,
            sources: vec!["a.txt".into()],
            mode: DeleteMode::Trash,
            on_error: ErrorPolicy::Ask,
            queue: None,
        }))
        .await
        .expect("queued");
    let Outcome::Job(job) = outcome else {
        panic!("a job, got {outcome:?}");
    };
    tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job.id))
        .await
        .expect("in time")
        .expect("finishes");
    assert!(!root.path().join("a.txt").exists());
    assert!(root.path().join("bin").join("a.txt").exists(), "in the bin");
}

/// The setting changes only the window's dialog: `ops.delete` into a bin that is not there
/// still answers `trash_unavailable`, for an agent's call too, and deletes nothing.
#[tokio::test]
async fn on_an_ops_delete_into_a_missing_bin_is_still_refused() {
    let (root, engine) = engine(false).await;
    switch_on(&engine).await;
    let refused = engine
        .dispatch(Command::OpsDelete(crate::DeleteRequest {
            panel: None,
            sources: vec!["a.txt".into()],
            mode: DeleteMode::Trash,
            on_error: ErrorPolicy::Ask,
            queue: None,
        }))
        .await
        .expect_err("no bin");
    assert_eq!(refused.code(), "trash_unavailable");
    assert!(root.path().join("a.txt").exists());
}

/// Servers have no bin; neither has a share by its form (only Windows paths have one).
#[tokio::test]
async fn servers_and_shares_have_no_bin() {
    let (_root, engine) = engine(true).await;
    let server = VfsPath {
        backend: BackendId::Sftp { connection_id: 1 },
        path: "/home".into(),
    };
    assert!(!engine.has_recycle_bin(&server));
    if cfg!(windows) {
        assert!(!engine.has_recycle_bin(&VfsPath::local(r"\\nas\share\folder")));
        assert!(!engine.has_recycle_bin(&VfsPath::local(r"\\wsl.localhost\Ubuntu\home")));
        assert!(engine.has_recycle_bin(&VfsPath::local(r"C:\Users")));
    }
}
