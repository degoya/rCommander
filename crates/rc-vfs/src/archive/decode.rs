//! The compressed streams around a tar or a single file - gzip, xz, bzip2 and zstd, all pure
//! Rust - behind a guard that counts both sides and stops an archive bomb as it unpacks.

use std::{
    io::{self, BufRead, BufReader, Read},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use ruzstd::decoding::{FrameDecoder, StreamingDecoder, errors::FrameDecoderError};

use super::{ArchiveLimits, Compression};

/// A decoder for `compression` over `inner`, without a guard (format detection reads only the
/// first block). Concatenated gzip members, xz streams, bzip2 streams and zstd frames are all
/// read, as the command-line tools do. An xz block whose dictionary passes the limits' decoder
/// memory fails with `OutOfMemory` before anything is set aside.
pub(crate) fn decoder<R: Read + 'static>(
    compression: Compression,
    inner: R,
    limits: &ArchiveLimits,
) -> io::Result<Box<dyn Read>> {
    Ok(match compression {
        Compression::Gzip => Box::new(flate2::read::MultiGzDecoder::new(inner)),
        Compression::Xz => Box::new(lzma_rust2::XzReader::new_mem_limit(
            inner,
            true,
            limits.decoder_memory_kib(),
        )),
        Compression::Bzip2 => Box::new(bzip2::read::MultiBzDecoder::new(inner)),
        Compression::Zstd => Box::new(ZstdFrames::new(inner)?),
    })
}

/// A guarded decoder: the unpacked bytes may not pass the limits' size, nor - beyond the ratio
/// threshold - the ratio to the packed bytes read so far.
pub(crate) fn guarded<R: Read + 'static>(
    compression: Compression,
    inner: R,
    limits: ArchiveLimits,
    path: &Path,
) -> io::Result<Guard> {
    let packed = Arc::new(AtomicU64::new(0));
    let counting = Counting {
        inner,
        count: Arc::clone(&packed),
    };
    Ok(Guard {
        inner: decoder(compression, counting, &limits)?,
        packed,
        unpacked: 0,
        limits,
        path: path.to_path_buf(),
    })
}

/// Counts the bytes read through it.
struct Counting<R> {
    inner: R,
    count: Arc<AtomicU64>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.count.fetch_add(read as u64, Ordering::Relaxed);
        Ok(read)
    }
}

/// The decoder with its limits. A decoder reads ahead, so the packed count runs ahead of what
/// the unpacked bytes needed: the ratio errs on the lenient side, never the strict one.
pub(crate) struct Guard {
    inner: Box<dyn Read>,
    packed: Arc<AtomicU64>,
    unpacked: u64,
    limits: ArchiveLimits,
    path: PathBuf,
}

impl Read for Guard {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf).map_err(|error| {
            // Raised as an archive error, so a crate on top (tar) cannot turn it into another.
            if error.kind() == io::ErrorKind::OutOfMemory {
                return crate::VfsError::ArchiveDecoderMemory {
                    path: self.path.clone(),
                }
                .into_io();
            }
            error
        })?;
        self.unpacked = self.unpacked.saturating_add(read as u64);
        let packed = self.packed.load(Ordering::Relaxed);
        self.limits
            .check_stream(self.unpacked, packed, &self.path)
            .map_err(crate::VfsError::into_io)?;
        Ok(read)
    }
}

/// Every frame of a zstd stream; `StreamingDecoder` alone stops after the first.
struct ZstdFrames<R: Read> {
    decoder: Option<StreamingDecoder<BufReader<R>, FrameDecoder>>,
}

impl<R: Read> ZstdFrames<R> {
    fn new(inner: R) -> io::Result<Self> {
        let decoder = StreamingDecoder::new(BufReader::new(inner)).map_err(zstd_error)?;
        Ok(Self {
            decoder: Some(decoder),
        })
    }
}

impl<R: Read> Read for ZstdFrames<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let Some(decoder) = self.decoder.as_mut() else {
                return Ok(0);
            };
            let read = decoder.read(buf)?;
            if read > 0 || buf.is_empty() {
                return Ok(read);
            }
            // This frame is done; another may follow.
            if decoder.get_mut().fill_buf()?.is_empty() {
                self.decoder = None;
                return Ok(0);
            }
            let Some(finished) = self.decoder.take() else {
                return Ok(0);
            };
            let next = StreamingDecoder::new(finished.into_inner()).map_err(zstd_error)?;
            self.decoder = Some(next);
        }
    }
}

fn zstd_error(error: FrameDecoderError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
