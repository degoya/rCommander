//! A folder's `descript.ion` on a server (RC-110-24): written beside the old file and put in its
//! place by a rename, against a server in memory whose writes and renames can be made to fail.

use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};

use async_trait::async_trait;
use rc_vfs::{BackendId, MemoryFs, Vfs, VfsError, VfsPath, WriteMode, WriteStream};
use tokio::io::{AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

use super::{change, store};
use crate::DESCRIPTION_FILE;

const DIR: &str = "/docs";
const FILE: &str = "/docs/descript.ion";

/// Two comments in OEM 850, which is no valid UTF-8.
const OEM: &[u8] = b"alt.txt gr\x94\xe1e\r\nnotes.txt old\r\n\"my file.txt\" \x84\x81\r\n";
const OEM_CHANGED: &[u8] = b"alt.txt gr\x94\xe1e\r\nnotes.txt new\r\n\"my file.txt\" \x84\x81\r\n";

/// A stream that takes nothing: the connection went while the file was written.
struct Lost(WriteStream);

impl AsyncWrite for Lost {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::Error::other("the connection is lost")))
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

/// A server in memory. Without `posix` its rename refuses a name that is taken, as SFTP
/// version 3 and many FTP servers do, and nothing replaces in one step.
struct Server {
    fs: MemoryFs,
    posix: bool,
    /// Every write breaks off after the file was created.
    lost: AtomicBool,
    /// Every rename is refused.
    no_rename: AtomicBool,
}

impl Server {
    /// A server holding the folder and, if given, its `descript.ion`.
    async fn with(posix: bool, comments: Option<&[u8]>) -> Arc<Self> {
        let fs = MemoryFs::new();
        fs.mkdir(Path::new(DIR), true).await.expect("mkdir");
        let server = Self {
            fs,
            posix,
            lost: AtomicBool::new(false),
            no_rename: AtomicBool::new(false),
        };
        if let Some(bytes) = comments {
            server.put(FILE, bytes).await;
        }
        Arc::new(server)
    }

    async fn put(&self, path: &str, bytes: &[u8]) {
        let mut file = self
            .fs
            .open_write(Path::new(path), WriteMode::Truncate)
            .await
            .expect("open");
        file.write_all(bytes).await.expect("write");
        file.shutdown().await.expect("close");
    }

    async fn bytes(&self, path: &Path) -> Option<Vec<u8>> {
        let mut stream = self.fs.open_read(path).await.ok()?;
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.expect("read");
        Some(bytes)
    }

    /// The names in the folder, sorted.
    async fn names(&self) -> Vec<String> {
        let entries = self.fs.list(Path::new(DIR)).await.expect("list");
        let mut names: Vec<String> = entries.into_iter().map(|entry| entry.name).collect();
        names.sort();
        names
    }

    fn refused(&self, to: &Path) -> rc_vfs::Result<()> {
        if self.no_rename.load(Ordering::SeqCst) {
            return Err(VfsError::PermissionDenied {
                path: to.to_path_buf(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl rc_vfs::testing::Forward for Server {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    async fn open_write(&self, path: &Path, mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        let stream = self.fs.open_write(path, mode).await?;
        if self.lost.load(Ordering::SeqCst) {
            return Ok(Box::new(Lost(stream)));
        }
        Ok(stream)
    }
    async fn rename(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        if self.fs.stat(to).await.is_ok() {
            return Err(VfsError::AlreadyExists {
                path: to.to_path_buf(),
            });
        }
        self.refused(to)?;
        self.fs.rename(from, to).await
    }
    async fn rename_replacing(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        if self.posix {
            self.refused(to)?;
            return self.fs.rename(from, to).await;
        }
        rc_vfs::testing::Forward::rename(self, from, to).await
    }
}

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Sftp { connection_id: 7 },
        path: PathBuf::from(path),
    }
}

/// The one temporary file the folder holds beside its `descript.ion`, if any.
async fn temporary(server: &Server) -> Option<PathBuf> {
    let mut others = server.names().await;
    others.retain(|name| name != DESCRIPTION_FILE);
    assert!(others.len() <= 1, "one temporary file at most: {others:?}");
    let name = others.pop()?;
    assert!(rc_ops::temp_name::is_temp_name(&name), "{name}");
    Some(Path::new(DIR).join(name))
}

#[tokio::test]
async fn a_code_page_file_on_a_server_changes_in_one_comment_only() {
    for posix in [true, false] {
        let server = Server::with(posix, Some(OEM)).await;
        let vfs: Arc<dyn Vfs> = server.clone();
        let comments = change(&vfs, &on_server(DIR), "notes.txt", "new")
            .await
            .expect("set");
        assert_eq!(comments.get("notes.txt").as_deref(), Some("new"));
        let stored = server.bytes(Path::new(FILE)).await;
        assert_eq!(stored.as_deref(), Some(OEM_CHANGED), "posix: {posix}");
        assert_eq!(server.names().await, [DESCRIPTION_FILE], "posix: {posix}");
    }
}

#[tokio::test]
async fn the_first_comment_on_a_server_leaves_no_temporary_file() {
    let server = Server::with(false, None).await;
    let vfs: Arc<dyn Vfs> = server.clone();
    change(&vfs, &on_server(DIR), "notes.txt", "first")
        .await
        .expect("set");
    let stored = server.bytes(Path::new(FILE)).await;
    assert_eq!(stored.as_deref(), Some(&b"notes.txt first\r\n"[..]));
    assert_eq!(server.names().await, [DESCRIPTION_FILE]);
}

/// The connection goes while the new file is written: the old one was never touched.
#[tokio::test]
async fn a_failed_write_on_a_server_leaves_the_old_file() {
    for posix in [true, false] {
        let server = Server::with(posix, Some(OEM)).await;
        server.lost.store(true, Ordering::SeqCst);
        let file = on_server(FILE);
        let failed = store(&*server, &file, OEM_CHANGED).await;
        assert_eq!(failed.expect_err("lost").code(), "io");
        let stored = server.bytes(Path::new(FILE)).await;
        assert_eq!(stored.as_deref(), Some(OEM), "posix: {posix}");
        assert_eq!(server.names().await, [DESCRIPTION_FILE], "posix: {posix}");
    }
}

/// A rename that replaces in one step and is refused: the old file is whole, the new one gone.
#[tokio::test]
async fn a_refused_replacing_rename_leaves_the_old_file() {
    let server = Server::with(true, Some(OEM)).await;
    server.no_rename.store(true, Ordering::SeqCst);
    let file = on_server(FILE);
    let failed = store(&*server, &file, OEM_CHANGED).await;
    assert_eq!(failed.expect_err("refused").code(), "permission_denied");
    let stored = server.bytes(Path::new(FILE)).await;
    assert_eq!(stored.as_deref(), Some(OEM));
    assert_eq!(server.names().await, [DESCRIPTION_FILE]);
}

/// A server that only removes and then renames, and refuses the rename: the old file is gone,
/// so the complete new one stays under its temporary name, and the error names that file.
#[tokio::test]
async fn a_refused_rename_after_the_removal_keeps_the_new_file_and_says_where() {
    let server = Server::with(false, Some(OEM)).await;
    server.no_rename.store(true, Ordering::SeqCst);
    let file = on_server(FILE);
    let failed = store(&*server, &file, OEM_CHANGED)
        .await
        .expect_err("refused");
    assert_eq!(
        server.bytes(Path::new(FILE)).await,
        None,
        "the old file went"
    );
    let temp = temporary(&server).await.expect("the new file stays");
    assert_eq!(server.bytes(&temp).await.as_deref(), Some(OEM_CHANGED));
    assert_eq!(failed.code(), "io");
    let VfsError::Io { path, .. } = &failed else {
        panic!("an i/o error expected, got {failed:?}");
    };
    assert_eq!(path, &temp);
    assert!(failed.to_string().contains("permission denied"), "{failed}");
}

/// What a failed run left is not the folder's comments: the next comment starts a new
/// `descript.ion` and leaves the file where it is.
#[tokio::test]
async fn a_leftover_temporary_file_is_not_read_as_the_comments() {
    let server = Server::with(false, Some(OEM)).await;
    server.no_rename.store(true, Ordering::SeqCst);
    let file = on_server(FILE);
    let _ = store(&*server, &file, OEM_CHANGED).await;
    let temp = temporary(&server).await.expect("left");
    server.no_rename.store(false, Ordering::SeqCst);

    let vfs: Arc<dyn Vfs> = server.clone();
    let comments = change(&vfs, &on_server(DIR), "other.txt", "fresh")
        .await
        .expect("set");
    assert_eq!(comments.get("notes.txt"), None);
    let stored = server.bytes(Path::new(FILE)).await;
    assert_eq!(stored.as_deref(), Some(&b"other.txt fresh\r\n"[..]));
    assert_eq!(server.bytes(&temp).await.as_deref(), Some(OEM_CHANGED));
    assert_eq!(server.names().await.len(), 2);
}

/// Memory is no server: written in place, as a device is, and no temporary file ever appears.
#[tokio::test]
async fn a_backend_that_is_neither_disk_nor_server_is_written_in_place() {
    let server = Server::with(false, Some(OEM)).await;
    server.no_rename.store(true, Ordering::SeqCst);
    let file = VfsPath {
        backend: BackendId::Memory(7),
        path: PathBuf::from(FILE),
    };
    store(&*server, &file, OEM_CHANGED).await.expect("written");
    let stored = server.bytes(Path::new(FILE)).await;
    assert_eq!(stored.as_deref(), Some(OEM_CHANGED));
    assert_eq!(server.names().await, [DESCRIPTION_FILE]);
}
