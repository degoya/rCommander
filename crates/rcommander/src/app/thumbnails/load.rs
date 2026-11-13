//! Loading one tile, off the UI thread: from the disk cache, else from the file - read whole
//! through the engine's `Vfs` (a server's only up to [`REMOTE_MAX_BYTES`]) and decoded on a
//! blocking thread - or from Explorer. At most [`AT_ONCE`] tiles load at a time, and a tile no
//! longer on screen when its turn comes is skipped: the newest screen counts.
//!
//! Explorer's handler is another program's code and may never answer: its call has
//! [`EXPLORER_LIMIT`], after which the tile shows its icon and the turn is free again. The
//! thread of such a call cannot be stopped, so at most [`EXPLORER_CALLS`] run or hang at a time;
//! past them Explorer is not asked until one came back (RC-110-29).

use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, AtomicUsize, Ordering},
    },
    time::Duration,
};

use iced::widget::image::Handle;
use rc_engine::Engine;
use tokio::sync::Semaphore;

use super::{
    decode::{self, Tile},
    disk::DiskCache,
    explorer,
    key::{REMOTE_MAX_BYTES, Source, ThumbKey},
};
use crate::{
    app::viewer::picture,
    recover::{LockOrRecover as _, LogPanic as _},
};

/// How many tiles load at once.
pub const AT_ONCE: usize = 2;

/// How long Explorer's handler may take for one tile.
const EXPLORER_LIMIT: Duration = Duration::from_secs(10);
/// Explorer calls running at once, those past their time included: the loading ones and four
/// that hang.
const EXPLORER_CALLS: usize = AT_ONCE + 4;
/// The Explorer calls that have not come back.
static EXPLORER_RUNNING: AtomicUsize = AtomicUsize::new(0);
/// The tiles written to disk in this run, for the trim every [`DiskCache::TRIM_EVERY`] of them.
static DISK_WRITES: AtomicU32 = AtomicU32::new(0);

/// What became of one tile.
#[derive(Debug, Clone)]
pub enum Loaded {
    /// The tile and the bytes of its pixels.
    Tile(Handle, usize),
    /// None: the icon stands in.
    None,
    /// Passed over: no longer on screen when its turn came.
    Skipped,
}

/// The tiles on screen now, which the loads look at when their turn comes.
#[derive(Debug, Clone, Default)]
pub struct Wanted(Arc<Mutex<HashSet<ThumbKey>>>);

impl Wanted {
    /// `keys` are the tiles on screen now.
    pub fn set(&self, keys: HashSet<ThumbKey>) {
        *self.0.lock_or_recover("thumbnails.wanted") = keys;
    }

    pub fn has(&self, key: &ThumbKey) -> bool {
        self.0.lock_or_recover("thumbnails.wanted").contains(key)
    }
}

/// One tile to load and what it needs.
pub struct Job {
    pub key: ThumbKey,
    pub source: Source,
    pub engine: Engine,
    /// The disk cache, while it is on; only a local file's tile goes there.
    pub disk: Option<DiskCache>,
    /// The bytes the disk cache may hold (`thumbnails.cache_mb`).
    pub disk_limit: u64,
    pub wanted: Wanted,
    pub gate: Arc<Semaphore>,
}

/// The tile of `job`, in its turn.
pub async fn load(job: Job) -> Loaded {
    let gate = Arc::clone(&job.gate);
    let Ok(_turn) = gate.acquire().await else {
        return Loaded::Skipped;
    };
    if !job.wanted.has(&job.key) {
        return Loaded::Skipped;
    }
    match tile(job).await {
        Some(tile) => {
            let bytes = tile.rgba.len();
            Loaded::Tile(Handle::from_rgba(tile.width, tile.height, tile.rgba), bytes)
        }
        None => Loaded::None,
    }
}

async fn tile(job: Job) -> Option<Tile> {
    let Job {
        key,
        source,
        disk,
        disk_limit,
        engine,
        ..
    } = job;
    let disk = disk_for(disk, &key);
    if let Some(disk) = disk.clone() {
        let key = key.clone();
        if let Some(tile) = blocking(move || disk.read(&key)).await {
            return Some(tile);
        }
    }
    let made = match source {
        Source::Picture => {
            let bytes = read_whole(&engine, &key).await?;
            let side = key.tile;
            blocking(move || decode::picture(&bytes, side)).await
        }
        Source::Explorer if key.local() => {
            let (path, side) = (key.path.path.clone(), key.tile);
            let call = move || explorer::thumbnail(&path, side);
            within(EXPLORER_LIMIT, &EXPLORER_RUNNING, EXPLORER_CALLS, call).await
        }
        Source::Explorer => None,
    }?;
    if let Some(disk) = disk {
        let (kept, key) = (made.clone(), key);
        // A tile not kept is made again next time; nothing else is lost.
        let _ = blocking(move || {
            disk.write(&key, &kept)
                .inspect_err(|error| tracing::debug!(%error, "a thumbnail was not kept"))
                .ok()?;
            // The cache grows while the program runs: it is trimmed as it does, not only at the
            // next start.
            disk.written(&DISK_WRITES, disk_limit)
        })
        .await;
    }
    Some(made)
}

/// The disk cache for the tile of `key`: only a local file's tile is kept there, never one of a
/// server, a device or an archive.
pub fn disk_for(disk: Option<DiskCache>, key: &ThumbKey) -> Option<DiskCache> {
    disk.filter(|_| key.local())
}

/// The whole file of `key`, through the engine: none past the size a tile reads.
async fn read_whole(engine: &Engine, key: &ThumbKey) -> Option<Vec<u8>> {
    let limit = if key.local() {
        picture::MAX_FILE_BYTES
    } else {
        REMOTE_MAX_BYTES
    };
    let file = engine.open_for_view(&key.path).await.ok()?;
    if file.size() == 0 || file.size() > limit {
        return None;
    }
    let length = usize::try_from(file.size()).ok()?;
    file.read_uncached(0, length).await.ok()
}

/// `work` on a blocking thread, given up after `limit`: `None` then, and the thread goes on
/// until `work` returns - `running` counts those that have not, and with `most` of them `work`
/// is not started at all.
async fn within<T: Send + 'static>(
    limit: Duration,
    running: &'static AtomicUsize,
    most: usize,
    work: impl FnOnce() -> Option<T> + Send + 'static,
) -> Option<T> {
    /// Takes the call off the count when it returns, or panics.
    struct Running(&'static AtomicUsize);
    impl Drop for Running {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::Relaxed);
        }
    }
    let taken = running.try_update(Ordering::Relaxed, Ordering::Relaxed, |now| {
        (now < most).then_some(now + 1)
    });
    if taken.is_err() {
        return None;
    }
    let call = blocking(move || {
        let _running = Running(running);
        work()
    });
    match tokio::time::timeout(limit, call).await {
        Ok(found) => found,
        Err(_) => {
            tracing::warn!("a thumbnail handler did not answer in time");
            None
        }
    }
}

/// `work` on a blocking thread; `None` when it found nothing or panicked (logged).
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Option<T> + Send + 'static,
) -> Option<T> {
    tokio::task::spawn_blocking(work)
        .await
        .log_panic("app.thumbnail")
        .ok()
        .flatten()
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;
