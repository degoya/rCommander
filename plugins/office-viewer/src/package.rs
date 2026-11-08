//! The zip package of an Office document and the parts read from it, by a reader of the plugin's
//! own: the end record ([`crate::entries`]), the central directory ([`crate::central`]), a local
//! header's name and extra field skipped, the data stored or raw Deflate (flate2 on miniz_oxide),
//! anything else refused. Every part within the [`Limits`]: the entries counted before the
//! central directory is read, the directory at most [`central::MAX_DIRECTORY_BYTES`], a part's
//! declared size and ratio checked before it is opened, its inflated bytes counted while it is
//! read ([`Capped`]), and its length and CRC-32 checked at its end ([`Checked`]). Part names are
//! matched case-insensitively, as Open Packaging names are. A part named `*.bin` - the macros of
//! `vbaProject.bin`, embedded objects, printer settings - is never opened.

use std::collections::BTreeMap;
use std::io::{self, Read, Seek, SeekFrom, Take};

use flate2::Crc;
use flate2::read::DeflateDecoder;

use crate::central::{self, DEFLATED, Entry, STORED};
use crate::entries::{self, le16};
use crate::limits::{Limits, code};
use crate::source_io::{SourceIo, carry, damaged, failure_of_io};
use crate::types::{Failure, FailureKind, Source};
use crate::xml::Xml;

const LOCAL_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
const LOCAL_BYTES: usize = 30;

/// One opened package.
pub(crate) struct Package<S: Source> {
    io: SourceIo<S>,
    size: u64,
    entries: Vec<Entry>,
    /// The index of each entry by its name in ASCII lower case.
    names: BTreeMap<String, usize>,
    limits: Limits,
}

/// One part being read.
pub(crate) type Part<'a, S> = Capped<Checked<Body<'a, S>>>;

impl<S: Source> Package<S> {
    /// Opens the zip in `src`: its end record, then its central directory.
    pub(crate) fn open(mut src: S, limits: Limits) -> Result<Self, Failure> {
        let end = entries::find(&mut src)?.ok_or_else(damaged)?;
        if end.declared > limits.max_entries {
            return Err(Failure::coded(FailureKind::BadData, code::TOO_MANY_ENTRIES));
        }
        let size = src.size();
        if end.size > central::MAX_DIRECTORY_BYTES
            || end
                .offset
                .checked_add(end.size)
                .is_none_or(|directory_end| directory_end > size)
        {
            return Err(damaged());
        }
        let mut io = SourceIo::new(src);
        io.seek(SeekFrom::Start(end.offset)).map_err(io_failure)?;
        let mut directory = vec![0u8; usize::try_from(end.size).map_err(|_| damaged())?];
        io.read_exact(&mut directory).map_err(io_failure)?;
        let entries = central::parse(&directory, end.entries)?;
        let names = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.name.to_ascii_lowercase(), index))
            .collect();
        Ok(Self {
            io,
            size,
            entries,
            names,
            limits,
        })
    }

    /// Whether the package has an entry `name`.
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.names.contains_key(&name.to_ascii_lowercase())
    }

    /// The part `name`, ready to read; `None` when there is none or it is a `*.bin` part, which
    /// is never read. A part declared larger than the limits is refused before it is opened.
    pub(crate) fn part(&mut self, name: &str) -> Result<Option<Part<'_, S>>, Failure> {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".bin") {
            return Ok(None);
        }
        let Some(entry) = self
            .names
            .get(&lower)
            .and_then(|index| self.entries.get(*index))
            .cloned()
        else {
            return Ok(None);
        };
        if entry.is_dir() {
            return Ok(None);
        }
        if entry.encrypted() || !matches!(entry.method, STORED | DEFLATED) {
            return Err(damaged());
        }
        check(&self.limits, entry.size, entry.packed)?;
        let start = self.data_start(&entry)?;
        self.io.seek(SeekFrom::Start(start)).map_err(io_failure)?;
        let raw = self.io.by_ref().take(entry.packed);
        let body = if entry.method == STORED {
            Body::Stored(raw)
        } else {
            Body::Deflated(Box::new(DeflateDecoder::new(raw)))
        };
        Ok(Some(Capped::new(
            Checked::new(body, entry.crc, entry.size),
            entry.packed,
            self.limits,
        )))
    }

    /// The part `name` as XML, as [`Package::part`] finds it.
    pub(crate) fn xml(&mut self, name: &str) -> Result<Option<Xml<Part<'_, S>>>, Failure> {
        let depth = self.limits.max_depth;
        Ok(self.part(name)?.map(|part| Xml::new(part, depth)))
    }

    /// Where the data of `entry` starts: after its local header's name and extra field, its
    /// packed bytes inside the file.
    fn data_start(&mut self, entry: &Entry) -> Result<u64, Failure> {
        self.io
            .seek(SeekFrom::Start(entry.header))
            .map_err(io_failure)?;
        let mut head = [0u8; LOCAL_BYTES];
        self.io.read_exact(&mut head).map_err(io_failure)?;
        if head.get(..4) != Some(&LOCAL_SIGNATURE[..]) {
            return Err(damaged());
        }
        let start = entry
            .header
            .checked_add(
                LOCAL_BYTES as u64 + u64::from(le16(&head, 26)) + u64::from(le16(&head, 28)),
            )
            .ok_or_else(damaged)?;
        if start
            .checked_add(entry.packed)
            .is_none_or(|data_end| data_end > self.size)
        {
            return Err(damaged());
        }
        Ok(start)
    }
}

/// A part's packed bytes, as they are or inflated.
pub(crate) enum Body<'a, S: Source> {
    Stored(Take<&'a mut SourceIo<S>>),
    Deflated(Box<DeflateDecoder<Take<&'a mut SourceIo<S>>>>),
}

impl<S: Source> Read for Body<'_, S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Stored(raw) => raw.read(buf),
            Self::Deflated(inflate) => inflate.read(buf),
        }
    }
}

/// A reader that checks a part against its directory entry: never more bytes than its size, and
/// at its end exactly its size and its CRC-32. A mismatch is a damaged part.
pub(crate) struct Checked<R: Read> {
    inner: R,
    crc: Crc,
    read: u64,
    size: u64,
    expected: u32,
}

impl<R: Read> Checked<R> {
    pub(crate) fn new(inner: R, expected: u32, size: u64) -> Self {
        Self {
            inner,
            crc: Crc::new(),
            read: 0,
            size,
            expected,
        }
    }
}

impl<R: Read> Read for Checked<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buf)?;
        if count == 0 {
            if !buf.is_empty() && (self.read != self.size || self.crc.sum() != self.expected) {
                return Err(carry(damaged()));
            }
            return Ok(0);
        }
        self.crc.update(buf.get(..count).unwrap_or_default());
        self.read += count as u64;
        if self.read > self.size {
            return Err(carry(damaged()));
        }
        Ok(count)
    }
}

/// A reader that inflates at most one byte past [`Limits::max_part_bytes`] - `take(limit + 1)`
/// on the bytes actually inflated, whatever the entry declared - and stops a part whose
/// inflated bytes pass [`Limits::max_ratio`] times its packed ones once past
/// [`Limits::ratio_floor`].
pub(crate) struct Capped<R: Read> {
    inner: R,
    inflated: u64,
    packed: u64,
    limits: Limits,
}

impl<R: Read> Capped<R> {
    pub(crate) fn new(inner: R, packed: u64, limits: Limits) -> Self {
        Self {
            inner,
            inflated: 0,
            packed,
            limits,
        }
    }
}

impl<R: Read> Read for Capped<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let room = (self.limits.max_part_bytes + 1).saturating_sub(self.inflated);
        let want = usize::try_from(room).unwrap_or(usize::MAX).min(buf.len());
        let count = self.inner.read(buf.get_mut(..want).unwrap_or_default())?;
        self.inflated += count as u64;
        check(&self.limits, self.inflated, self.packed).map_err(carry)?;
        Ok(count)
    }
}

/// Whether `inflated` bytes from `packed` ones stay within `limits`.
fn check(limits: &Limits, inflated: u64, packed: u64) -> Result<(), Failure> {
    if inflated > limits.max_part_bytes {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_LARGE));
    }
    if inflated > limits.ratio_floor && inflated > packed.saturating_mul(limits.max_ratio) {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_COMPRESSED));
    }
    Ok(())
}

/// A failure of reading the file: the host's or a limit's when one travels inside, else damaged.
fn io_failure(error: io::Error) -> Failure {
    failure_of_io(&error)
}

#[cfg(test)]
#[path = "package_tests.rs"]
mod tests;
