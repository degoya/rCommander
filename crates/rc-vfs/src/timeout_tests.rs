use std::{
    io,
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex, mpsc},
    task::{Context, Poll},
    time::{Duration, Instant, SystemTime},
};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

use super::{IoCancel, TimeoutFs, Timeouts};
use crate::{
    Capabilities, Entry, LocalFs, ReadStream, Result, SetAttributes, Vfs, VfsError, WriteMode,
    WriteStream, conformance,
};

/// A share whose server is gone: every call under `/share` waits forever; anything else answers
/// at once. A listing of a folder that answers takes `listing` (on tokio's clock).
struct Share {
    listing: Duration,
}

fn hangs(path: &Path) -> bool {
    path.starts_with("/share")
}

/// A folder's entry, as a backend that answers gives it (no literal: `Entry` grows fields).
async fn dir_entry() -> Entry {
    let fs = crate::MemoryFs::new();
    fs.mkdir(Path::new("/d"), false).await.expect("mkdir");
    fs.stat(Path::new("/d")).await.expect("stat")
}

/// A stream that never answers, and never wakes its task: only a time limit ends a wait on it.
struct Silent;

impl AsyncRead for Silent {
    fn poll_read(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        _buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Pending
    }
}

impl AsyncWrite for Silent {
    fn poll_write(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        _bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Pending
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Pending
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Pending
    }
}

async fn answer<T>(path: &Path, value: T) -> Result<T> {
    if hangs(path) {
        std::future::pending::<()>().await;
    }
    Ok(value)
}

#[async_trait]
impl Vfs for Share {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            rename_across_dirs: true,
            symlinks: false,
            case_sensitive: true,
            unix_mode: false,
            set_mtime: true,
            free_space: false,
        }
    }
    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        answer(dir, ()).await?;
        tokio::time::sleep(self.listing).await;
        Ok(Vec::new())
    }
    /// Two batches, the second after the listing time: a wrapper that lists at once sends one.
    async fn list_batches(
        &self,
        dir: &Path,
        batches: tokio::sync::mpsc::Sender<Vec<Entry>>,
    ) -> Result<()> {
        answer(dir, ()).await?;
        let _ = batches.send(vec![dir_entry().await]).await;
        tokio::time::sleep(self.listing).await;
        let _ = batches.send(vec![dir_entry().await]).await;
        Ok(())
    }
    async fn stat(&self, path: &Path) -> Result<Entry> {
        answer(path, ()).await?;
        Ok(dir_entry().await)
    }
    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        answer(path, Box::new(Silent) as ReadStream).await
    }
    async fn open_write(&self, path: &Path, _mode: WriteMode) -> Result<WriteStream> {
        answer(path, Box::new(Silent) as WriteStream).await
    }
    async fn mkdir(&self, path: &Path, _recursive: bool) -> Result<()> {
        answer(path, ()).await
    }
    async fn rename(&self, from: &Path, _to: &Path) -> Result<()> {
        answer(from, ()).await
    }
    async fn remove_file(&self, path: &Path) -> Result<()> {
        answer(path, ()).await
    }
    async fn remove_dir(&self, path: &Path, _recursive: bool) -> Result<()> {
        answer(path, ()).await?;
        tokio::time::sleep(self.listing).await;
        Ok(())
    }
    async fn set_mtime(&self, path: &Path, _modified: SystemTime) -> Result<()> {
        answer(path, ()).await
    }
    async fn set_attributes(&self, path: &Path, _change: SetAttributes) -> Result<()> {
        answer(path, ()).await
    }
}

const LIMITS: Timeouts = Timeouts {
    request: Duration::from_secs(20),
    listing: Duration::from_secs(300),
};

fn share(listing: Duration) -> TimeoutFs {
    TimeoutFs::new(Arc::new(Share { listing }), LIMITS)
}

fn code<T>(result: Result<T>) -> &'static str {
    result.map_or_else(|error| error.code(), |_| "ok")
}

#[tokio::test(start_paused = true)]
async fn a_call_that_never_answers_times_out_with_its_path() {
    let fs = share(Duration::ZERO);
    let error = fs.stat(Path::new("/share/a")).await.expect_err("no answer");
    assert!(
        matches!(&error, VfsError::TimedOut { path, seconds: 20 } if path == Path::new("/share/a")),
        "{error:?}"
    );
    assert_eq!(
        code(fs.mkdir(Path::new("/share/b"), true).await),
        "drive_timed_out"
    );
    assert_eq!(
        code(fs.open_read(Path::new("/share/c")).await),
        "drive_timed_out"
    );
    assert_eq!(
        code(
            fs.rename(Path::new("/share/d"), Path::new("/share/e"))
                .await
        ),
        "drive_timed_out"
    );
    // A folder that does not answer is found by its own `stat`, within the request limit.
    let started = tokio::time::Instant::now();
    assert_eq!(code(fs.list(Path::new("/share")).await), "drive_timed_out");
    let took = started.elapsed();
    assert!(
        took >= Duration::from_secs(20) && took < Duration::from_secs(21),
        "{took:?}"
    );
    assert_eq!(
        code(fs.remove_dir(Path::new("/share/f"), true).await),
        "drive_timed_out"
    );
}

#[tokio::test(start_paused = true)]
async fn a_listing_whose_folder_answers_gets_the_listing_limit() {
    // A long listing of a folder that answered: longer than a request may take.
    let fs = share(Duration::from_secs(120));
    assert_eq!(code(fs.list(Path::new("/home")).await), "ok");
    assert_eq!(code(fs.list_details(Path::new("/home")).await), "ok");
    assert_eq!(code(fs.remove_dir(Path::new("/home/x"), true).await), "ok");

    let fs = share(Duration::from_secs(400));
    let error = fs.list(Path::new("/home")).await.expect_err("too long");
    assert!(
        matches!(&error, VfsError::TimedOut { seconds: 300, .. }),
        "{error:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_listing_in_batches_passes_each_batch_on_within_the_listing_limit() {
    let fs = share(Duration::from_secs(120));
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    assert_eq!(
        code(fs.list_batches(Path::new("/home"), sender).await),
        "ok"
    );
    let mut sizes = Vec::new();
    while let Some(batch) = receiver.recv().await {
        sizes.push(batch.len());
    }
    assert_eq!(sizes, [1, 1], "the inner backend's batches, unchanged");

    let (sender, _receiver) = tokio::sync::mpsc::channel(4);
    assert_eq!(
        code(fs.list_batches(Path::new("/share"), sender).await),
        "drive_timed_out"
    );
    let fs = share(Duration::from_secs(400));
    let (sender, _receiver) = tokio::sync::mpsc::channel(4);
    let error = fs
        .list_batches(Path::new("/home"), sender)
        .await
        .expect_err("too long");
    assert!(
        matches!(&error, VfsError::TimedOut { seconds: 300, .. }),
        "{error:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn the_listing_reports_its_own_error_when_the_folder_answers_with_one() {
    let fs = TimeoutFs::new(Arc::new(crate::MemoryFs::new()), LIMITS);
    assert_eq!(code(fs.list(Path::new("/missing")).await), "not_found");
}

#[tokio::test(start_paused = true)]
async fn a_stream_that_stops_answering_times_out() {
    // The files outside `/share` open at once; their streams never answer.
    let fs = share(Duration::ZERO);
    let mut reader = fs.open_read(Path::new("/home/a.bin")).await.expect("opens");
    let failed = reader.read(&mut [0_u8; 16]).await.expect_err("no answer");
    assert_eq!(failed.kind(), io::ErrorKind::TimedOut);
    let back = VfsError::from_io(failed, "/elsewhere");
    assert!(
        matches!(&back, VfsError::TimedOut { path, .. } if path == Path::new("/home/a.bin")),
        "{back:?}"
    );
    let mut writer = fs
        .open_write(Path::new("/home/b.bin"), WriteMode::CreateNew)
        .await
        .expect("opens");
    let failed = writer.write(b"abc").await.expect_err("no answer");
    assert_eq!(failed.kind(), io::ErrorKind::TimedOut);
}

#[tokio::test(start_paused = true)]
async fn cancel_ends_the_calls_waiting_now_and_no_later_one() {
    let cancel = IoCancel::new();
    let fs = Arc::new(share(Duration::ZERO).with_cancel(cancel.clone()));
    assert!(!cancel.stalled(Duration::ZERO));
    let waiting = {
        let fs = Arc::clone(&fs);
        tokio::spawn(async move { fs.stat(Path::new("/share/a")).await })
    };
    for _ in 0..100 {
        if cancel.stalled(Duration::ZERO) {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(cancel.stalled(Duration::ZERO), "the call waits");
    assert_eq!(cancel.cancel(), 1);
    let ended = waiting.await.expect("the task ends");
    assert_eq!(code(ended), "cancelled");
    assert!(!cancel.stalled(Duration::ZERO), "nothing waits any more");
    assert_eq!(
        code(fs.stat(Path::new("/home")).await),
        "ok",
        "later calls run"
    );
    assert_eq!(cancel.cancel(), 0);
}

/// A backend whose `stat` blocks a thread until the test lets it go, as a system call on a
/// share that went away does.
struct Blocking {
    release: Arc<Mutex<mpsc::Receiver<()>>>,
}

#[async_trait]
impl Vfs for Blocking {
    fn capabilities(&self) -> Capabilities {
        crate::MemoryFs::new().capabilities()
    }
    async fn list(&self, _dir: &Path) -> Result<Vec<Entry>> {
        Ok(Vec::new())
    }
    async fn stat(&self, path: &Path) -> Result<Entry> {
        let release = Arc::clone(&self.release);
        let entry = dir_entry().await;
        tokio::task::spawn_blocking(move || {
            let _ = release.lock().map(|release| release.recv());
        })
        .await
        .map_err(|join| VfsError::from_io(io::Error::other(join), path))?;
        Ok(entry)
    }
    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        Err(VfsError::NotFound { path: path.into() })
    }
    async fn open_write(&self, path: &Path, _mode: WriteMode) -> Result<WriteStream> {
        Err(VfsError::NotFound { path: path.into() })
    }
    async fn mkdir(&self, _path: &Path, _recursive: bool) -> Result<()> {
        Ok(())
    }
    async fn rename(&self, _from: &Path, _to: &Path) -> Result<()> {
        Ok(())
    }
    async fn remove_file(&self, _path: &Path) -> Result<()> {
        Ok(())
    }
    async fn remove_dir(&self, _path: &Path, _recursive: bool) -> Result<()> {
        Ok(())
    }
    async fn set_mtime(&self, _path: &Path, _modified: SystemTime) -> Result<()> {
        Ok(())
    }
    async fn set_attributes(&self, _path: &Path, _change: SetAttributes) -> Result<()> {
        Ok(())
    }
}

/// On the real clock: the call returns at its limit although the thread it started still hangs.
#[tokio::test]
async fn the_caller_is_free_while_the_system_call_still_hangs() {
    let (let_go, release) = mpsc::channel();
    let inner = Blocking {
        release: Arc::new(Mutex::new(release)),
    };
    let limits = Timeouts {
        request: Duration::from_millis(50),
        listing: Duration::from_millis(50),
    };
    let fs = TimeoutFs::new(Arc::new(inner), limits);
    let started = Instant::now();
    let answer = fs.stat(Path::new("/share/a")).await;
    let took = started.elapsed();
    let _ = let_go.send(());
    assert_eq!(code(answer), "drive_timed_out");
    assert!(took < Duration::from_secs(5), "returned after {took:?}");
}

/// Around a backend that answers, the wrapper changes nothing.
#[tokio::test]
async fn around_the_local_backend_everything_works_as_before() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = TimeoutFs::new(Arc::new(LocalFs::new()), Timeouts::default());
    conformance::run(&fs, dir.path()).await;
    let file = dir.path().join("seek.bin");
    conformance::write_file(&fs, &file, WriteMode::CreateNew, b"hello").await;
    conformance::seekable_read_when_offered(&fs, &file).await;
}
