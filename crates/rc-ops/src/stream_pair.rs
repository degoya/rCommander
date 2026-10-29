//! The stream pair (RC-060-06): a file's read stream feeds its write stream through [`DEPTH`]
//! buffers, both sides at once. Reading the next buffer overlaps writing the last one, which is
//! what a copy from one server to another needs - no temporary file, never more than the
//! buffers in memory - and costs a copy to or from a disk nothing.
//!
//! The coupling is real back pressure: there are only [`DEPTH`] buffers, and the read side
//! fills one only once the write side gave it back, so a slow target stops the source from
//! reading on. Both sides work only against `rc_vfs` streams, so every backend - SFTP, FTP, an
//! archive as source - goes through here without a case of its own.
//!
//! How the two end ([`both`]): a failing write stops the read at once; a failing read lets the
//! write finish what was read before (a part file then holds every byte that came through, for
//! the resume), and the read's error is the outcome. A write that fails names the target as the
//! side that failed (`Failure::target`), as does a target that refuses to open the file or to
//! rename the complete part; a cancel stops both.
//!
//! A copy within one FTP server holds two of its transfer connections, one per stream; such
//! copies open their streams one at a time ([`Runner::one_ftp_pair`]), else two of them could
//! each hold one connection and wait for a second for ever.

use std::{
    io,
    path::Path,
    sync::{Arc, PoisonError},
};

use rc_vfs::{BackendId, VfsError, VfsPath};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex, MutexGuard, mpsc},
};

use crate::{
    OpsError,
    control::Control,
    runner::{Runner, Step},
    walk::Item,
};

/// Buffers between the read and the write side: with the queue's 256 KiB, one MiB in flight.
pub(crate) const DEPTH: usize = 4;

/// Held while both streams of a copy within one FTP server are open.
static FTP_PAIR: Mutex<()> = Mutex::const_new(());

/// The buffers the files of one job take turns with (RC-080-45): a stream pair takes its
/// buffers here and leaves them here when it ends, so a job of 100 000 small files makes its
/// [`DEPTH`] buffers once, not per file. Never more than [`DEPTH`] are kept.
#[derive(Clone, Default)]
pub(crate) struct Buffers(Arc<std::sync::Mutex<Vec<Vec<u8>>>>);

impl Buffers {
    /// A kept buffer of `size` bytes, else a new one.
    fn take(&self, size: usize) -> Vec<u8> {
        let kept = self.kept().pop().filter(|bytes| bytes.len() == size);
        kept.unwrap_or_else(|| vec![0_u8; size])
    }

    /// Keeps `bytes` for the next file, while fewer than [`DEPTH`] are kept.
    fn keep(&self, bytes: Vec<u8>) {
        let mut kept = self.kept();
        if kept.len() < DEPTH {
            kept.push(bytes);
        }
    }

    fn kept(&self) -> std::sync::MutexGuard<'_, Vec<Vec<u8>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A buffer and how much of it one read filled.
pub(crate) struct Chunk {
    bytes: Vec<u8>,
    len: usize,
}

impl Chunk {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// The read side: fills free buffers from the source and hands them on in order.
pub(crate) struct Feed {
    full: mpsc::Sender<Chunk>,
    free: mpsc::Receiver<Vec<u8>>,
    size: usize,
    made: usize,
    buffers: Buffers,
}

/// The write side: takes the filled buffers in order and gives each back once written.
pub(crate) struct Drain {
    full: mpsc::Receiver<Chunk>,
    free: mpsc::Sender<Vec<u8>>,
    buffers: Buffers,
}

/// A feed and its drain, with [`DEPTH`] buffers of `size` bytes between them, taken from
/// `buffers` as needed and left there again.
pub(crate) fn pair(size: usize, buffers: &Buffers) -> (Feed, Drain) {
    let (full_tx, full_rx) = mpsc::channel(DEPTH);
    let (free_tx, free_rx) = mpsc::channel(DEPTH);
    let feed = Feed {
        full: full_tx,
        free: free_rx,
        size: size.max(1),
        made: 0,
        buffers: buffers.clone(),
    };
    let drain = Drain {
        full: full_rx,
        free: free_tx,
        buffers: buffers.clone(),
    };
    (feed, drain)
}

/// The buffers given back and not taken again go back to the job's.
impl Drop for Feed {
    fn drop(&mut self) {
        while let Ok(bytes) = self.free.try_recv() {
            self.buffers.keep(bytes);
        }
    }
}

/// The buffers filled and not written - after a failing write - go back to the job's.
impl Drop for Drain {
    fn drop(&mut self) {
        self.full.close();
        while let Ok(chunk) = self.full.try_recv() {
            self.buffers.keep(chunk.bytes);
        }
    }
}

impl Feed {
    /// Reads `reader` to its end into the buffers. Stops without error once the drain is gone
    /// (the write side ended; it has the reason); a cancel stops it as cancelled.
    pub(crate) async fn fill(
        mut self,
        reader: &mut rc_vfs::ReadStream,
        control: &Control,
        from: &Path,
    ) -> Result<(), Step> {
        loop {
            let Some(mut bytes) = self.buffer().await else {
                return Ok(());
            };
            let read = control.or_cancel(reader.read(&mut bytes)).await?;
            let len = read.map_err(|error| OpsError::from_io(error, from))?;
            if len == 0 {
                return Ok(());
            }
            if self.full.send(Chunk { bytes, len }).await.is_err() {
                return Ok(());
            }
        }
    }

    /// A free buffer: one given back, a new one while fewer than [`DEPTH`] exist, else the next
    /// one given back; `None` once the drain is gone.
    async fn buffer(&mut self) -> Option<Vec<u8>> {
        if let Ok(bytes) = self.free.try_recv() {
            return Some(bytes);
        }
        if self.made < DEPTH {
            self.made += 1;
            return Some(self.buffers.take(self.size));
        }
        self.free.recv().await
    }
}

impl Drain {
    /// The next filled buffer; `None` once the feed ended and every buffer was taken.
    pub(crate) async fn next(&mut self) -> Option<Chunk> {
        self.full.recv().await
    }

    /// Gives a written buffer back to the feed; once the feed ended, to the job's buffers.
    pub(crate) fn give_back(&self, chunk: Chunk) {
        // Never full: no more than `DEPTH` buffers exist.
        if let Err(refused) = self.free.try_send(chunk.bytes) {
            self.buffers.keep(refused.into_inner());
        }
    }
}

/// Runs the read side `read` and the write side `write` at once: the write's outcome, unless the
/// read failed. A failing write ends both at once; after a failing read the write takes what
/// was read before, then the read's error counts.
pub(crate) async fn both<T>(
    read: impl Future<Output = Result<(), Step>>,
    write: impl Future<Output = Result<T, Step>>,
) -> Result<T, Step> {
    tokio::pin!(read, write);
    let mut ended = None;
    loop {
        tokio::select! {
            biased;
            done = &mut read, if ended.is_none() => ended = Some(done),
            written = &mut write => {
                let written = written?;
                // The write ends only once the feed is gone, so the read has ended too.
                let read = match ended {
                    Some(read) => read,
                    None => read.as_mut().await,
                };
                read?;
                return Ok(written);
            }
        }
    }
}

impl Runner {
    /// Streams `reader` into `writer` through the stream pair; the source's checksum when the
    /// job verifies (`hasher` holds what came before, for a continued file), and the bytes
    /// written.
    pub(crate) async fn pump(
        &mut self,
        reader: &mut rc_vfs::ReadStream,
        writer: &mut rc_vfs::WriteStream,
        from: &Path,
        to: &Path,
        hasher: blake3::Hasher,
    ) -> Result<(blake3::Hash, u64), Step> {
        let (feed, mut drain) = pair(self.buffer_size, &self.buffers);
        let control = self.job.control.clone();
        let read = feed.fill(reader, &control, from);
        let write = self.drain_into(&mut drain, writer, to, hasher);
        both(read, write).await
    }

    /// One file's streams to their end, the same for a fresh copy and a continued part: the
    /// [`Runner::pump`], the writer shut down - after a failure too, whose error then counts -,
    /// both streams and the FTP pair (`pair`, [`Runner::one_ftp_pair`]) let go, and
    /// [`Runner::check_copy`] on what landed at `to`, where `before` bytes were already.
    pub(crate) async fn land(
        &mut self,
        item: &Item,
        streams: (rc_vfs::ReadStream, rc_vfs::WriteStream),
        pair: Option<MutexGuard<'static, ()>>,
        to: &Path,
        hasher: blake3::Hasher,
        before: u64,
    ) -> Result<(), Step> {
        let (mut reader, mut writer) = streams;
        let pumped = match self
            .pump(&mut reader, &mut writer, &item.path, to, hasher)
            .await
        {
            Ok(pumped) => match writer.shutdown().await {
                Ok(()) => Ok(pumped),
                Err(error) => Err(self.target_failed(error, to)),
            },
            Err(step) => {
                let _ = writer.shutdown().await;
                Err(step)
            }
        };
        drop(writer);
        drop(reader);
        drop(pair);
        let (hash, written) = pumped?;
        self.check_copy(item, to, &hash, before + written).await
    }

    /// The write side: every buffer of `drain` into `writer`, with a pause and cancel check
    /// before each, counted as progress.
    async fn drain_into(
        &mut self,
        drain: &mut Drain,
        writer: &mut rc_vfs::WriteStream,
        to: &Path,
        mut hasher: blake3::Hasher,
    ) -> Result<(blake3::Hash, u64), Step> {
        let mut written = 0_u64;
        loop {
            let next = self.job.control.or_cancel(drain.next()).await?;
            let Some(chunk) = next else {
                return Ok((hasher.finalize(), written));
            };
            self.checkpoint().await?;
            let bytes = chunk.bytes();
            if self.options.verify {
                hasher.update(bytes);
            }
            let wrote = self.job.control.or_cancel(writer.write_all(bytes)).await?;
            if let Err(error) = wrote {
                return Err(self.target_failed(error, to));
            }
            written += bytes.len() as u64;
            self.progress.count(bytes.len() as u64);
            self.tick(false);
            drain.give_back(chunk);
        }
    }

    /// A write to the target that failed at `to`: its error, with the target noted as the side
    /// that failed.
    pub(crate) fn target_failed(&mut self, error: io::Error, to: &Path) -> Step {
        self.note_target(to);
        OpsError::from_io(error, to).into()
    }

    /// A call on the target that refused at `at` - the part file not opened, the complete part
    /// not renamed: its error, with the target noted as the side that failed.
    pub(crate) fn target_refused(&mut self, error: VfsError, at: &Path) -> Step {
        self.note_target(at);
        error.into()
    }

    fn note_target(&mut self, at: &Path) {
        self.failed_on = self.dst_id.clone().map(|backend| VfsPath {
            backend,
            path: at.to_path_buf(),
        });
    }

    /// Before a copy within one FTP server opens its two streams: waits until no other such copy
    /// has its streams open. `None` for every other copy; a cancel meanwhile stops the job.
    pub(crate) async fn one_ftp_pair(&self) -> Result<Option<MutexGuard<'static, ()>>, Step> {
        let within_ftp = matches!(self.src_id, BackendId::Ftp { .. })
            && self.dst_id.as_ref() == Some(&self.src_id);
        if !within_ftp {
            return Ok(None);
        }
        Ok(Some(self.job.control.or_cancel(FTP_PAIR.lock()).await?))
    }
}

#[cfg(test)]
#[path = "stream_pair_tests.rs"]
mod tests;
