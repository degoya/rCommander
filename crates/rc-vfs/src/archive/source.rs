//! The archive file, read from a blocking task through the outer backend's async streams.
//!
//! The archive crates are synchronous; every scan and every read runs as one blocking task,
//! and these readers drive the outer [`Vfs`] from it with [`Handle::block_on`]. The seekable
//! one keeps a buffer and moves inside it without touching the stream, so the many small reads
//! and seeks of a zip central directory stay cheap. A watched source fails its reads once the
//! reader of the entry it is read for went away ([`Watch`]).

use std::{
    error::Error,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    runtime::Handle,
};

use super::pipe::Watch;
use crate::{ReadStream, Result, SeekStream, Vfs, VfsError};

/// Bytes the seekable reader fetches at once.
const BUFFER: usize = 64 * 1024;

/// The outer archive file and the runtime that reads it.
#[derive(Clone)]
pub(crate) struct Source {
    handle: Handle,
    vfs: Arc<dyn Vfs>,
    path: PathBuf,
    watch: Watch,
}

impl Source {
    /// Must be called on the runtime (in an async function), before the blocking task starts.
    pub(crate) fn new(vfs: Arc<dyn Vfs>, path: &Path) -> Result<Self> {
        let handle = Handle::try_current().map_err(|error| VfsError::Io {
            path: path.to_path_buf(),
            source: io::Error::other(error),
        })?;
        Ok(Self {
            handle,
            vfs,
            path: path.to_path_buf(),
            watch: Watch::default(),
        })
    }

    /// The same source, its readers stopping once the reader `watch` follows is gone.
    pub(crate) fn watched(mut self, watch: Watch) -> Self {
        self.watch = watch;
        self
    }

    /// The archive file's path on its backend.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Opens the archive for random access (zip, 7z, plain tar). Blocks.
    pub(crate) fn seekable(&self) -> Result<SeekReader> {
        let stream = self
            .handle
            .block_on(self.vfs.open_read_seekable(&self.path))?;
        SeekReader::open(self.handle.clone(), stream, self.watch.clone())
            .map_err(|error| io_error(error, &self.path))
    }

    /// Opens the archive as a stream (compressed tars, single files, detection). Blocks.
    pub(crate) fn stream(&self) -> Result<StreamReader> {
        let stream = self.handle.block_on(self.vfs.open_read(&self.path))?;
        Ok(StreamReader {
            handle: self.handle.clone(),
            stream,
            watch: self.watch.clone(),
        })
    }
}

/// The archive as a plain stream.
pub(crate) struct StreamReader {
    handle: Handle,
    stream: ReadStream,
    watch: Watch,
}

impl Read for StreamReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.watch.check()?;
        self.handle.block_on(self.stream.read(buf))
    }
}

/// The archive with random access, buffered.
pub(crate) struct SeekReader {
    handle: Handle,
    stream: SeekStream,
    /// The file's length, for seeks from the end.
    len: u64,
    /// Where the caller is.
    pos: u64,
    /// Where the stream is.
    stream_pos: u64,
    /// Bytes from `buffer_start` on.
    buffer: Vec<u8>,
    buffer_start: u64,
    watch: Watch,
}

impl SeekReader {
    pub(crate) fn open(handle: Handle, mut stream: SeekStream, watch: Watch) -> io::Result<Self> {
        let len = handle.block_on(stream.seek(SeekFrom::End(0)))?;
        handle.block_on(stream.seek(SeekFrom::Start(0)))?;
        Ok(Self {
            handle,
            stream,
            len,
            pos: 0,
            stream_pos: 0,
            buffer: Vec::new(),
            buffer_start: 0,
            watch,
        })
    }

    /// The buffered bytes from the caller's position on, if it is inside the buffer.
    fn buffered(&self) -> Option<&[u8]> {
        let offset = usize::try_from(self.pos.checked_sub(self.buffer_start)?).ok()?;
        self.buffer.get(offset..).filter(|rest| !rest.is_empty())
    }

    /// Reads from the stream at the caller's position, seeking it there first if needed.
    fn read_stream(&mut self, into: &mut [u8]) -> io::Result<usize> {
        self.watch.check()?;
        if self.stream_pos != self.pos {
            self.stream_pos = self
                .handle
                .block_on(self.stream.seek(SeekFrom::Start(self.pos)))?;
        }
        let read = self.handle.block_on(self.stream.read(into))?;
        self.stream_pos += read as u64;
        Ok(read)
    }
}

impl Read for SeekReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(rest) = self.buffered() {
            let count = rest.len().min(buf.len());
            buf[..count].copy_from_slice(&rest[..count]);
            self.pos += count as u64;
            return Ok(count);
        }
        if buf.len() >= BUFFER {
            // A large read goes straight into the caller's buffer.
            let count = self.read_stream(buf)?;
            self.pos += count as u64;
            return Ok(count);
        }
        let mut buffer = std::mem::take(&mut self.buffer);
        buffer.resize(BUFFER, 0);
        let filled = match self.read_stream(&mut buffer) {
            Ok(filled) => filled,
            Err(error) => {
                buffer.clear();
                self.buffer = buffer;
                return Err(error);
            }
        };
        buffer.truncate(filled);
        self.buffer = buffer;
        self.buffer_start = self.pos;
        let count = filled.min(buf.len());
        buf[..count].copy_from_slice(&self.buffer[..count]);
        self.pos += count as u64;
        Ok(count)
    }
}

impl Seek for SeekReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::End(delta) => self.len.checked_add_signed(delta),
            SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
        };
        // Only the position moves; the stream follows on the next read outside the buffer.
        self.pos = target.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before the start of the archive",
            )
        })?;
        Ok(self.pos)
    }
}

/// Maps an I/O error from an archive crate or the outer backend. A limit or password error the
/// guard raised stays itself, also when a crate wrapped it on the way; invalid or truncated
/// data is a damaged archive, and a decoder that cannot have its memory (over the limit, or
/// refused by the system) is `archive_decoder_memory`.
pub(crate) fn io_error(error: io::Error, path: &Path) -> VfsError {
    if let Some(found) = wrapped(&error) {
        return found;
    }
    // Decoders say their input is corrupt with InvalidData, lzma-rust2 with InvalidInput (as
    // after a wrong 7z password).
    match error.kind() {
        io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput | io::ErrorKind::UnexpectedEof => {
            VfsError::ArchiveDamaged {
                path: path.to_path_buf(),
                reason: error.to_string(),
            }
        }
        io::ErrorKind::OutOfMemory => VfsError::ArchiveDecoderMemory {
            path: path.to_path_buf(),
        },
        _ => VfsError::from_io(error, path),
    }
}

/// An archive error somewhere in the chain of `error`, rebuilt (a `VfsError` is not `Clone`).
fn wrapped(error: &io::Error) -> Option<VfsError> {
    let mut current: Option<&(dyn Error + 'static)> =
        error.get_ref().map(|inner| inner as &(dyn Error + 'static));
    while let Some(inner) = current {
        if let Some(found) = inner.downcast_ref::<VfsError>() {
            return match found {
                VfsError::PasswordRequired { path } => {
                    Some(VfsError::PasswordRequired { path: path.clone() })
                }
                VfsError::PasswordWrong { path } => {
                    Some(VfsError::PasswordWrong { path: path.clone() })
                }
                VfsError::ArchiveLimit { path, limit } => Some(VfsError::ArchiveLimit {
                    path: path.clone(),
                    limit,
                }),
                VfsError::ArchiveDecoderMemory { path } => {
                    Some(VfsError::ArchiveDecoderMemory { path: path.clone() })
                }
                VfsError::ArchiveDamaged { path, reason } => Some(VfsError::ArchiveDamaged {
                    path: path.clone(),
                    reason: reason.clone(),
                }),
                _ => None,
            };
        }
        // `io::Error::source` skips the error it wraps; step into it by hand.
        current = match inner
            .downcast_ref::<io::Error>()
            .and_then(io::Error::get_ref)
        {
            Some(wrapped) => Some(wrapped as &(dyn Error + 'static)),
            None => inner.source(),
        };
    }
    None
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
