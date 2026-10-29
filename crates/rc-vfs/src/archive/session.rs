//! Reading sessions (RC-080-46): a producer thread that keeps the opened archive - the reader and
//! the format's index (a zip's central directory, a 7z header) - between the reads of its
//! entries, and walks a solid 7z block or a tar stream on from where the last entry ended. Reading
//! a whole archive in its own order unpacks it once instead of once per entry.
//!
//! A producer serves one read at a time. Once an entry went out whole it parks the way in
//! ([`Parked`]) in the archive's slot and waits; the next read takes it from there and sends its
//! request. A read while the producer is busy starts a producer of its own, as every read did
//! before. A producer ends when its reader went away (a cancel), when a read failed, when nobody
//! asked for [`IDLE`], or when its archive is gone; what it keeps while parked is the decoder
//! the archive's limits admitted for the read before, never more.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError, Weak,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::Duration,
};

use super::{
    Stamp,
    pipe::{self, PipeWriter, Pumped, Watch},
};
use crate::{ReadStream, Result, VfsError};

/// How long a parked producer waits for the next read before it ends.
pub(crate) const IDLE: Duration = Duration::from_secs(15);

/// The entry a read wants.
#[derive(Debug, Clone)]
pub(crate) struct Wanted {
    /// The format's index (zip, 7z) or position (tar) of the entry.
    pub index: u64,
    pub size: u64,
    pub path: PathBuf,
    /// The entry is encrypted: a zip reads it with the password.
    pub encrypted: bool,
}

/// One read, sent to a producer.
pub(crate) struct Request {
    pub wanted: Wanted,
    pub writer: PipeWriter,
    /// The way in, handed back to the slot once this entry went out whole.
    way: Sender<Request>,
}

impl Request {
    /// Ends the read with `error`.
    pub(crate) fn fail(self, error: VfsError) {
        self.writer.fail(error);
    }
}

/// A producer waiting for its next read.
struct Parked {
    id: u64,
    stamp: Stamp,
    way: Sender<Request>,
}

/// What a producer runs: the format's loop over the requests its feed hands it.
pub(crate) type Serve = Box<dyn FnOnce(Request, &mut Feed) + Send>;

/// The reading sessions of one archive: the slot a producer parks in.
#[derive(Default)]
pub(crate) struct Sessions {
    slot: Arc<Mutex<Option<Parked>>>,
    ids: AtomicU64,
}

impl Sessions {
    /// A stream of `wanted`, from the parked producer when one waits for the archive as `stamp`
    /// describes it, else from a new one running `serve`.
    pub(crate) async fn open(
        &self,
        stamp: Stamp,
        wanted: Wanted,
        serve: Serve,
    ) -> Result<ReadStream> {
        if let Some(way) = self.take(stamp) {
            let (writer, reader) = pipe::pair(wanted.size);
            let request = Request {
                wanted: wanted.clone(),
                writer,
                way: way.clone(),
            };
            if way.send(request).is_ok() {
                drop(way);
                if let Ok(stream) = pipe::opened(reader, &wanted.path).await {
                    return Ok(stream);
                }
                // A parked stream that went stale, or an error the fresh start below meets again.
            }
        }
        let path = wanted.path.clone();
        let (writer, reader) = pipe::pair(wanted.size);
        let (way, inbox) = mpsc::channel();
        let request = Request {
            wanted,
            writer,
            way,
        };
        let mut feed = Feed {
            inbox,
            slot: Arc::downgrade(&self.slot),
            stamp,
            id: self.ids.fetch_add(1, Ordering::Relaxed),
            watch: Watch::default(),
        };
        // A thread of its own, not the blocking pool: a parked producer must not hold up the
        // runtime's shutdown nor take a pool thread while it waits.
        thread::Builder::new()
            .name("rc-archive-read".to_owned())
            .spawn(move || {
                feed.watch.follow(&request.writer);
                serve(request, &mut feed);
            })
            .map_err(|error| VfsError::from_io(error, &path))?;
        pipe::opened(reader, &path).await
    }

    /// The way to the producer parked for `stamp`; one parked for an archive file that changed
    /// since is let go.
    fn take(&self, stamp: Stamp) -> Option<Sender<Request>> {
        let parked = lock(&self.slot).take()?;
        (parked.stamp == stamp).then_some(parked.way)
    }
}

/// A producer's side: the requests after the first, the slot it parks in.
pub(crate) struct Feed {
    inbox: Receiver<Request>,
    slot: Weak<Mutex<Option<Parked>>>,
    stamp: Stamp,
    id: u64,
    watch: Watch,
}

impl Feed {
    /// What the producer's archive reads check, to stop once the current reader is gone.
    pub(crate) fn watch(&self) -> Watch {
        self.watch.clone()
    }

    /// Ends `request` with `outcome`: the next request once the entry went out whole, `None` when
    /// the producer ends - its reader went away, the read failed (told to the reader), or nobody
    /// asked within [`IDLE`].
    pub(crate) fn finish(&mut self, request: Request, outcome: Result<Pumped>) -> Option<Request> {
        match outcome {
            Ok(Pumped::Complete) => {}
            Ok(Pumped::Abandoned { .. }) => return None,
            Err(error) => {
                request.fail(error);
                return None;
            }
        }
        // The writer goes first: its reader sees the end only once no sender is left.
        let Request { writer, way, .. } = request;
        drop(writer);
        if !self.park(way) {
            return None;
        }
        let next = self.wait()?;
        self.watch.follow(&next.writer);
        Some(next)
    }

    /// Parks the way in; `false` when the archive is gone or another producer parked first.
    fn park(&self, way: Sender<Request>) -> bool {
        let Some(slot) = self.slot.upgrade() else {
            return false;
        };
        let mut slot = lock(&slot);
        if slot.is_some() {
            return false;
        }
        *slot = Some(Parked {
            id: self.id,
            stamp: self.stamp,
            way,
        });
        true
    }

    /// The next request; `None` after [`IDLE`] without one, or once nobody can send one.
    fn wait(&self) -> Option<Request> {
        loop {
            match self.inbox.recv_timeout(IDLE) {
                Ok(request) => return Some(request),
                Err(RecvTimeoutError::Disconnected) => return None,
                Err(RecvTimeoutError::Timeout) => {
                    let slot = self.slot.upgrade()?;
                    let mut slot = lock(&slot);
                    if slot.as_ref().is_some_and(|parked| parked.id == self.id) {
                        *slot = None;
                        return None;
                    }
                    // Taken: a request is on its way, or its sender is dropped (disconnected).
                }
            }
        }
    }
}

#[cfg(test)]
impl Feed {
    /// A feed with nowhere to park: its producer ends after the first request.
    pub(crate) fn lonely() -> Self {
        let (_, inbox) = mpsc::channel();
        Self {
            inbox,
            slot: Weak::new(),
            stamp: Stamp {
                size: 0,
                modified: None,
            },
            id: 0,
            watch: Watch::default(),
        }
    }

    /// The request for `wanted` and the reader of its stream, its archive reads watching it.
    pub(crate) fn request(&self, wanted: Wanted) -> (Request, pipe::PipeReader) {
        let (writer, reader) = pipe::pair(wanted.size);
        let (way, _) = mpsc::channel();
        self.watch.follow(&writer);
        (
            Request {
                wanted,
                writer,
                way,
            },
            reader,
        )
    }
}

fn lock(slot: &Mutex<Option<Parked>>) -> MutexGuard<'_, Option<Parked>> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The error of an entry that is not where the format looked.
pub(crate) fn not_found(path: &Path) -> VfsError {
    VfsError::NotFound {
        path: path.to_path_buf(),
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
