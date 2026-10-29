use std::path::{Path, PathBuf};

use rc_vfs::{ArchiveFormat, BackendId};

use super::{entry_name, opens_on_enter, shown_path};
use crate::test_support;
use crate::{Command, Engine, VfsPath, settings::DECODER_MEMORY_KEY};

/// An archive fixture of rc-vfs (`tests/fixtures/archives/`, written by its `generate.py`).
pub(crate) fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives")
        .join(name)
}

/// A folder holding a copy of the fixture `name`, and an engine whose panels show the folder.
pub(crate) async fn engine_beside(name: &str) -> (tempfile::TempDir, Engine) {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::copy(fixture(name), root.path().join(name)).expect("copy the fixture");
    let engine = test_support::headless(root.path()).await;
    (root, engine)
}

#[test]
fn enter_opens_archives_by_extension_only() {
    for name in [
        "a.zip",
        "B.ZIP",
        "a.7z",
        "a.tar",
        "a.tar.gz",
        "a.tgz",
        "a.tar.xz",
        "a.tar.bz2",
        "a.tar.zst",
        "notes.txt.gz",
    ] {
        assert!(opens_on_enter(name), "{name}");
    }
    for name in [
        "report.docx",
        "tool.jar",
        "zip",
        ".zip",
        "a.zipx",
        "photo.jpg",
    ] {
        assert!(!opens_on_enter(name), "{name}");
    }
}

#[test]
fn a_location_inside_an_archive_shows_as_a_path_through_the_archive_file() {
    let outer = VfsPath::local("/data/backup.zip");
    let root = VfsPath::archive(outer.clone(), ArchiveFormat::Zip, "/");
    assert_eq!(shown_path(&root), PathBuf::from("/data/backup.zip"));
    assert_eq!(
        shown_path(&root.join("docs").join("sub")),
        PathBuf::from("/data/backup.zip/docs/sub")
    );
    assert_eq!(shown_path(&outer), PathBuf::from("/data/backup.zip"));
}

#[test]
fn the_root_of_an_archive_is_named_after_the_archive_file() {
    let root = VfsPath::archive(VfsPath::local("/data/a.7z"), ArchiveFormat::SevenZ, "/");
    assert_eq!(entry_name(&root).as_deref(), Some("a.7z"));
    assert_eq!(entry_name(&root.join("docs")).as_deref(), Some("docs"));
    assert_eq!(entry_name(&VfsPath::local("/")), None);
}

#[tokio::test]
async fn a_typed_path_through_an_archive_file_leads_into_it() {
    let (root, engine) = engine_beside("tree.zip").await;
    let typed = VfsPath::local(root.path().join("tree.zip").join("docs"));
    let inside = engine
        .through_archive(&typed)
        .await
        .expect("readable")
        .expect("through an archive");
    assert!(matches!(inside.backend, BackendId::Archive { .. }));
    assert_eq!(inside.path, Path::new("/").join("docs"));
    assert_eq!(shown_path(&inside), typed.path);

    let plain = VfsPath::local(root.path().join("missing").join("docs"));
    assert_eq!(
        engine.through_archive(&plain).await.expect("readable"),
        None
    );
}

#[tokio::test]
async fn an_archive_backend_is_opened_once_and_reused() {
    let (root, engine) = engine_beside("tree.7z").await;
    let file = VfsPath::local(root.path().join("tree.7z"));
    let archive = engine
        .archive_root(&file)
        .await
        .expect("readable")
        .expect("an archive");
    let first = engine.archive_fs(&archive.backend).expect("opens");
    let again = engine.archive_fs(&archive.backend).expect("opens");
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    let nested = VfsPath::archive(archive.join("inner.zip"), ArchiveFormat::Zip, "/");
    assert_eq!(
        engine
            .archive_fs(&nested.backend)
            .map(|_| ())
            .map_err(|e| e.code()),
        Err("unsupported")
    );
}

/// RC-080-35: `archive.decoder_memory_mib` reaches the archives the engine opens; a change
/// closes the open ones, so the next look opens them under the new limit.
#[tokio::test]
async fn the_decoder_memory_setting_reaches_the_archives_opened() {
    let (root, engine) = engine_beside("tree.7z").await;
    assert!(
        engine.attach_db(rc_db::Db::open_in_memory().await.expect("db")),
        "a database"
    );
    let file = VfsPath::local(root.path().join("tree.7z"));
    let archive = engine
        .archive_root(&file)
        .await
        .expect("readable")
        .expect("an archive");
    assert_eq!(
        engine.inner.archives.limits().max_decoder_memory,
        256 << 20,
        "the default"
    );
    let first = engine.archive_fs(&archive.backend).expect("opens");
    engine
        .dispatch(Command::SetSetting {
            key: DECODER_MEMORY_KEY.into(),
            value: serde_json::json!(64),
        })
        .await
        .expect("stored");
    assert_eq!(engine.inner.archives.limits().max_decoder_memory, 64 << 20);
    assert_eq!(
        engine.inner.ops.config().archive.limits.max_decoder_memory,
        64 << 20,
        "unpacking takes it too"
    );
    let again = engine.archive_fs(&archive.backend).expect("opens anew");
    assert!(!std::sync::Arc::ptr_eq(&first, &again));
    let refused = engine
        .dispatch(Command::SetSetting {
            key: DECODER_MEMORY_KEY.into(),
            value: serde_json::json!(8),
        })
        .await
        .map(|_| ())
        .map_err(|error| error.code());
    assert_eq!(refused, Err("setting_invalid"), "below 16 MiB");
}
