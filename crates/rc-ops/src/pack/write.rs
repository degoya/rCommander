//! The blocking side of a pack: the archive file, the format's writer, and the sources pulled
//! through their backend one buffer at a time.
//!
//! [`Feed`] opens each source with the runtime's [`Handle`] (this runs in a blocking task) and
//! reads it in buffers; before each buffer it waits while the job is paused and stops once it is
//! cancelled, and after each it reports the bytes read for the progress. A source that turns out
//! shorter than it was when counted fails the archive instead of corrupting it (a tar header has
//! stated its size already); one that grew goes in with its counted size and is listed in
//! [`Grown`], so it is a failure and a move to the archive keeps it.

use std::{
    fmt,
    fs::{File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::SystemTime,
};

use rc_vfs::{ReadStream, Vfs};
use tokio::{io::AsyncReadExt, runtime::Handle, sync::watch};

use super::{PackFormat, sevenz, tar, zip};
use crate::control::Control;

/// Bytes per read of a source.
const BUFFER: usize = 256 * 1024;

/// What one archive member is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MemberKind {
    Dir,
    File,
    /// A link stored as a link, with its target.
    Link(String),
}

/// One member of the archive: where its data comes from and what is stored about it.
#[derive(Debug, Clone)]
pub(crate) struct Member {
    /// The source on its backend.
    pub path: PathBuf,
    /// The name in the archive, `/`-separated.
    pub name: String,
    pub kind: MemberKind,
    /// The size when counted; a file must still have it.
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub mode: Option<u32>,
    pub readonly: bool,
}

/// One archive to write.
pub(crate) struct Spec {
    pub members: Vec<Member>,
    pub format: PackFormat,
    /// 0 to 9.
    pub level: u8,
    pub solid: bool,
    /// The temporary file, created new.
    pub temp: PathBuf,
}

/// The source being read, shared with the job for its progress.
#[derive(Debug, Clone, Default)]
pub(crate) struct Current(Arc<Mutex<Option<AtWork>>>);

/// The member being read: its source, its size, and the bytes the feed had read before it.
#[derive(Debug, Clone)]
pub(crate) struct AtWork {
    pub path: PathBuf,
    pub size: u64,
    pub from: u64,
}

impl Current {
    pub(crate) fn get(&self) -> Option<AtWork> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set(&self, member: &Member, from: u64) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(AtWork {
            path: member.path.clone(),
            size: member.size,
            from,
        });
    }
}

/// The sources that held more than their counted size: only that much of them went in.
#[derive(Debug, Clone, Default)]
pub(crate) struct Grown(Arc<Mutex<Vec<PathBuf>>>);

impl Grown {
    pub(crate) fn take(&self) -> Vec<PathBuf> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn push(&self, path: PathBuf) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(path);
    }
}

/// The sources, as the blocking writer reads them.
pub(crate) struct Feed {
    handle: Handle,
    src: Arc<dyn Vfs>,
    control: Control,
    /// Bytes read so far, for the progress.
    bytes: watch::Sender<u64>,
    current: Current,
    grown: Grown,
}

impl Feed {
    /// Must be called on the runtime, before the blocking task starts.
    pub(crate) fn new(
        src: Arc<dyn Vfs>,
        control: Control,
        bytes: watch::Sender<u64>,
    ) -> io::Result<Self> {
        Ok(Self {
            handle: Handle::try_current().map_err(io::Error::other)?,
            src,
            control,
            bytes,
            current: Current::default(),
            grown: Grown::default(),
        })
    }

    pub(crate) fn current(&self) -> Current {
        self.current.clone()
    }

    pub(crate) fn grown(&self) -> Grown {
        self.grown.clone()
    }

    /// Fails once the job is cancelled; waits while it is paused.
    fn checkpoint(&self) -> io::Result<()> {
        if self.control.is_paused() {
            self.handle
                .block_on(self.control.checkpoint())
                .map_err(|_| cancelled())?;
        }
        if self.control.is_cancelled() {
            return Err(cancelled());
        }
        Ok(())
    }

    /// Opens the member's source, buffered; it must yield exactly `member.size` bytes.
    pub(crate) fn open(&self, member: &Member) -> io::Result<BufReader<Source<'_>>> {
        self.checkpoint()?;
        self.current.set(member, *self.bytes.borrow());
        let stream = self
            .handle
            .block_on(self.src.open_read(&member.path))
            .map_err(rc_vfs::VfsError::into_io)?;
        let mut source = Source {
            feed: self,
            stream,
            path: member.path.clone(),
            left: member.size,
        };
        if source.left == 0 {
            source.check_end()?;
        }
        Ok(BufReader::with_capacity(BUFFER, source))
    }
}

fn cancelled() -> io::Error {
    io::Error::other("the job was cancelled")
}

/// One source, read through its backend.
pub(crate) struct Source<'a> {
    feed: &'a Feed,
    stream: ReadStream,
    path: PathBuf,
    /// Bytes still to come.
    left: u64,
}

impl Read for Source<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 || buf.is_empty() {
            return Ok(0);
        }
        self.feed.checkpoint()?;
        let want = buf
            .len()
            .min(BUFFER)
            .min(usize::try_from(self.left).unwrap_or(BUFFER));
        let read = self
            .feed
            .handle
            .block_on(self.stream.read(&mut buf[..want]))?;
        if read == 0 {
            return Err(rc_vfs::VfsError::Io {
                path: self.path.clone(),
                source: io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the file got shorter while it was packed",
                ),
            }
            .into_io());
        }
        self.left -= read as u64;
        self.feed.bytes.send_modify(|bytes| *bytes += read as u64);
        if self.left == 0 {
            self.check_end()?;
        }
        Ok(read)
    }
}

impl Source<'_> {
    /// Past the counted size: one more byte means the file grew since it was counted.
    fn check_end(&mut self) -> io::Result<()> {
        let mut byte = [0_u8; 1];
        let more = self.feed.handle.block_on(self.stream.read(&mut byte))?;
        if more > 0 {
            self.feed.grown.push(self.path.clone());
        }
        Ok(())
    }
}

/// Writes the archive `spec` describes into its temporary file, flushed to disk.
pub(crate) fn write(spec: &Spec, feed: &Feed) -> io::Result<()> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&spec.temp)?;
    let level = u32::from(spec.level);
    let file = match spec.format {
        PackFormat::Zip => zip::write(BufWriter::new(file), &spec.members, level, feed)?,
        PackFormat::SevenZ => {
            sevenz::write(BufWriter::new(file), &spec.members, level, spec.solid, feed)?
        }
        PackFormat::TarZst => zst(file, spec, feed)?,
        format => tar::write_compressed(BufWriter::new(file), format, level, &spec.members, feed)?,
    };
    file.sync_all()
}

/// tar.zst: the tar is written on a second thread into a pipe that ruzstd's encoder reads.
fn zst(file: File, spec: &Spec, feed: &Feed) -> io::Result<File> {
    let produce = |pipe: PipeWriter| {
        // Whole buffers into the pipe, not one message per header or read.
        let mut out =
            tar::write_plain(BufWriter::with_capacity(BUFFER, pipe), &spec.members, feed)?;
        out.flush()
    };
    encode_zst(BufWriter::new(file), spec.level, produce)?
        .into_inner()
        .map_err(io::IntoInnerError::into_error)
}

/// Compresses what `produce` writes into the pipe it is given, as zstd into `out`; `out` back.
///
/// The encoder unwraps every I/O error it meets, so neither side ever hands it one: the pipe
/// ends early instead, and the errors come back from the producer and the sticky writer. A
/// panic of the producer thread comes back as [`panicked`]; one of the encoder unwinds this
/// (blocking) thread, which the job sees as a failed task - neither ends the app.
pub(super) fn encode_zst<W, P>(out: W, level: u8, produce: P) -> io::Result<W>
where
    W: Write,
    P: FnOnce(PipeWriter) -> io::Result<()> + Send,
{
    let level = if level == 0 {
        ruzstd::encoding::CompressionLevel::Uncompressed
    } else {
        ruzstd::encoding::CompressionLevel::Fastest
    };
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(4);
    let mut sink = Sticky {
        inner: out,
        error: None,
        stop: Arc::clone(&stop),
    };
    let produced = std::thread::scope(|scope| {
        let producer = scope.spawn(move || produce(PipeWriter { tx }));
        let reader = PipeReader {
            rx,
            chunk: Vec::new(),
            offset: 0,
            stop,
        };
        ruzstd::encoding::compress(reader, &mut sink, level);
        // Joined here, so a panic there is an answer, not a panic of the scope.
        producer.join()
    });
    // The output's own failure first: the producer then only saw the pipe close.
    if let Some(error) = sink.error {
        return Err(error);
    }
    produced.map_err(|_| panicked())??;
    sink.flush_inner()?;
    Ok(sink.inner)
}

/// A writer thread of an archive crate panicked: the archive failed, the app goes on.
#[derive(Debug)]
pub(crate) struct WriterPanicked;

impl fmt::Display for WriterPanicked {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the archive writer stopped unexpectedly")
    }
}

impl std::error::Error for WriterPanicked {}

/// The error a panicked writer leaves.
pub(crate) fn panicked() -> io::Error {
    io::Error::other(WriterPanicked)
}

/// Whether `error` is what a panicked writer left.
pub(crate) fn is_panic(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<WriterPanicked>())
}

/// The tar side of the zstd pipe; fails once the encoder is gone.
pub(super) struct PipeWriter {
    tx: mpsc::SyncSender<Vec<u8>>,
}

impl Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tx
            .send(buf.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "the encoder stopped"))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The encoder side of the zstd pipe; ends (never fails) when the tar writer is done or the
/// output failed.
struct PipeReader {
    rx: mpsc::Receiver<Vec<u8>>,
    chunk: Vec<u8>,
    offset: usize,
    stop: Arc<AtomicBool>,
}

impl Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.offset >= self.chunk.len() {
            if self.stop.load(Ordering::Relaxed) {
                return Ok(0);
            }
            match self.rx.recv() {
                Ok(chunk) => {
                    self.chunk = chunk;
                    self.offset = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let count = buf.len().min(self.chunk.len() - self.offset);
        buf[..count].copy_from_slice(&self.chunk[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

/// The output under the zstd encoder: keeps the first error and swallows the rest, and
/// tells the pipe to end.
struct Sticky<W> {
    inner: W,
    error: Option<io::Error>,
    stop: Arc<AtomicBool>,
}

impl<W: Write> Sticky<W> {
    /// The last flush, with its error.
    fn flush_inner(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write> Write for Sticky<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.error.is_none()
            && let Err(error) = self.inner.write_all(buf)
        {
            self.error = Some(error);
            self.stop.store(true, Ordering::Relaxed);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.error.is_none()
            && let Err(error) = self.inner.flush()
        {
            self.error = Some(error);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
