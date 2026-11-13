use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use rc_vfs::{BackendId, MemoryFs};
use tokio::io::AsyncWriteExt as _;

use super::*;

#[test]
fn a_part_file_sits_beside_its_target() {
    assert_eq!(
        part_path(Path::new("/srv/data/report.pdf")),
        Path::new("/srv/data/report.pdf.rcpart")
    );
    assert_eq!(part_path(Path::new("/a")), Path::new("/a.rcpart"));
}

#[test]
fn only_servers_are_remote() {
    assert!(is_remote(&BackendId::Sftp { connection_id: 3 }));
    assert!(is_remote(&BackendId::Ftp { connection_id: 3 }));
    assert!(!is_remote(&BackendId::Local));
    assert!(!is_remote(&BackendId::Memory(1)));
}

/// RC-080-37: a file out of an archive goes through a part onto a disk or a server, but starts
/// over instead of resuming; into memory it is written in place.
#[test]
fn staged_and_resumable_are_apart() {
    let archive = BackendId::Archive {
        outer: Box::new(rc_vfs::VfsPath::local("/a.zip")),
        format: rc_vfs::ArchiveFormat::Zip,
    };
    let server = BackendId::Sftp { connection_id: 3 };
    assert!(is_staged(&BackendId::Local));
    assert!(is_staged(&server));
    assert!(!is_staged(&BackendId::Memory(1)));
    assert!(!is_resumable(&archive, &BackendId::Local));
    assert!(is_resumable(&archive, &server));
    assert!(is_resumable(&BackendId::Local, &BackendId::Local));
    assert!(is_resumable(&server, &BackendId::Memory(1)));
    assert!(!is_resumable(&BackendId::Memory(1), &BackendId::Memory(2)));
}

/// A file of this machine keeps its whole mode; a server's, a device's or an archive's loses
/// set-uid, set-gid, sticky and everyone's write - with no umask in the way.
#[test]
fn a_foreign_source_keeps_no_special_bits_and_no_write_for_everyone() {
    assert_eq!(kept_mode(0o4755, &BackendId::Local), 0o4755);
    assert_eq!(foreign_mode_under(0o4755, 0), 0o755);
    assert_eq!(foreign_mode_under(0o2775, 0), 0o775);
    assert_eq!(foreign_mode_under(0o1777, 0), 0o775);
    assert_eq!(foreign_mode_under(0o666, 0), 0o664);
    assert_eq!(foreign_mode_under(0o600, 0), 0o600);
    for src in [
        BackendId::Sftp { connection_id: 1 },
        BackendId::Ftp { connection_id: 1 },
    ] {
        assert_eq!(kept_mode(0o4755, &src), foreign_mode(0o4755), "{src:?}");
    }
}

/// RC-080-70: a file unpacked or downloaded with mode 0775 has 0755 under umask 022, 0700 under
/// 077; a file of this machine keeps its mode, as `cp -p` does.
#[test]
fn a_foreign_mode_is_held_to_the_umask() {
    assert_eq!(foreign_mode_under(0o775, 0o022), 0o755);
    assert_eq!(foreign_mode_under(0o775, 0o077), 0o700);
    assert_eq!(foreign_mode_under(0o4777, 0o002), 0o775);
    assert_eq!(foreign_mode(0o775), 0o775 & !rc_vfs::umask());
    let archive = BackendId::Archive {
        outer: Box::new(rc_vfs::VfsPath::local("/a.zip")),
        format: rc_vfs::ArchiveFormat::Zip,
    };
    assert_eq!(kept_mode(0o775, &archive), foreign_mode(0o775));
    assert_eq!(kept_mode(0o775, &BackendId::Local), 0o775);
}

/// A server as SFTP version 3 is one: its rename never replaces, and - `full` - refuses to
/// rename onto a name that is free, as a quota reached after the old file went would.
struct Version3 {
    fs: MemoryFs,
    full: AtomicBool,
    /// The server has a rename that replaces.
    posix: bool,
}

impl Version3 {
    async fn with(posix: bool) -> Self {
        let fs = MemoryFs::new();
        for (path, bytes) in [("/f.txt.rcpart", b"new"), ("/f.txt", b"old")] {
            let mut file = fs
                .open_write(Path::new(path), WriteMode::CreateNew)
                .await
                .expect("open");
            file.write_all(bytes).await.expect("write");
            file.shutdown().await.expect("close");
        }
        Self {
            fs,
            full: AtomicBool::new(false),
            posix,
        }
    }

    async fn holds(&self, path: &str) -> Option<u64> {
        self.fs.stat(Path::new(path)).await.ok().map(|at| at.size)
    }
}

#[async_trait]
impl rc_vfs::testing::Forward for Version3 {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    async fn rename(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        if self.fs.stat(to).await.is_ok() {
            return Err(VfsError::AlreadyExists {
                path: to.to_path_buf(),
            });
        }
        if self.full.load(Ordering::SeqCst) {
            return Err(VfsError::PermissionDenied {
                path: to.to_path_buf(),
            });
        }
        self.fs.rename(from, to).await
    }
    async fn rename_replacing(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        if self.posix {
            return self.fs.rename(from, to).await;
        }
        rc_vfs::testing::Forward::rename(self, from, to).await
    }
}

const PART: &str = "/f.txt.rcpart";
const TARGET: &str = "/f.txt";

/// RC-110-25: where the rename does not replace, the old target goes first and the file takes
/// its name.
#[tokio::test]
async fn a_rename_that_does_not_replace_removes_the_target_first() {
    let server = Version3::with(false).await;
    replace_by_rename(&server, Path::new(PART), Path::new(TARGET))
        .await
        .expect("replaced");
    assert_eq!(server.holds(TARGET).await, Some(3));
    assert_eq!(server.holds(PART).await, None);
}

/// RC-110-25, the failure of the second rename: the old target is gone, and the caller is told
/// that the file it wanted to rename is the only copy left - which is still there.
#[tokio::test]
async fn a_failed_rename_after_the_removal_says_the_target_is_gone() {
    let server = Version3::with(false).await;
    server.full.store(true, Ordering::SeqCst);
    let unreplaced = replace_by_rename(&server, Path::new(PART), Path::new(TARGET))
        .await
        .expect_err("refused");
    assert!(unreplaced.target_gone);
    assert_eq!(unreplaced.error.code(), "permission_denied");
    assert_eq!(server.holds(TARGET).await, None, "the old file went");
    assert_eq!(server.holds(PART).await, Some(3), "the new one stays");
}

/// A rename that replaces is one step: nothing is removed first, even where the plain rename
/// would refuse.
#[tokio::test]
async fn a_rename_that_replaces_never_removes_the_target() {
    let server = Version3::with(true).await;
    server.full.store(true, Ordering::SeqCst);
    replace_by_rename(&server, Path::new(PART), Path::new(TARGET))
        .await
        .expect("replaced");
    assert_eq!(server.holds(TARGET).await, Some(3));
    assert_eq!(server.holds(PART).await, None);
}

/// A refusal before anything was removed leaves both files, and says so.
#[tokio::test]
async fn a_missing_file_fails_with_the_target_still_there() {
    let server = Version3::with(false).await;
    let unreplaced = replace_by_rename(&server, Path::new("/none.rcpart"), Path::new("/free.txt"))
        .await
        .expect_err("not found");
    assert!(!unreplaced.target_gone);
    assert_eq!(server.holds(TARGET).await, Some(3));
}
