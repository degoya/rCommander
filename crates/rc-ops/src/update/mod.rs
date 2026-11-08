//! Changing an archive (RC-110-05): entries added to and removed from a local zip, 7z or plain
//! tar, Total Commander's F5 into an archive and F8 inside one.
//!
//! Never in place. The new archive is written under a temporary name beside the old one
//! (`temp_name::pack_path`), checked with the program's own reader when `verify` is on, and only
//! then renamed onto the old one - after the archive file was found unchanged since the update
//! began (`archive_changed`) and its reading sessions were let go ([`ArchiveRelease`]); a file
//! still open elsewhere is `archive_in_use`. A failure, a cancel or a crash leaves the old archive
//! byte for byte, and at most the temporary file.
//!
//! What is kept is copied as it is stored: a zip's entries raw (no inflate, the CRC and sizes
//! stay), a tar's entries with their extension headers byte for byte (long names, PAX times), a
//! 7z's blocks raw. Only a 7z block that loses an entry is decoded - under the decoder memory of
//! `archive.decoder_memory_mib` - and encoded again without it. New entries are packed as a pack
//! packs them (`pack/`), into the folder `under`.
//!
//! Refused: compressed tars and single compressed files (`archive_not_modifiable`, as Total
//! Commander refuses them), a zip behind a prefix (a self-extractor), a tar with sparse files,
//! encrypted entries or headers (`archive_encrypted`: no password is asked for to write), archives
//! that are not on a local drive (`archive_not_local`), a read-only archive file
//! (`archive_file_read_only`), too little room for the new archive beside the old one
//! (`not_enough_space`). Entries are removed by the format's index, never by a shown name.

mod run;
mod sevenz;
mod sevenz_pipe;
mod tar;
mod zip;

use std::{
    collections::BTreeSet,
    fmt,
    fs::{File, OpenOptions},
    io::{self, BufWriter, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use rc_vfs::{ArchiveFormat, ArchiveLimits, BackendId, VfsPath};
use serde::{Deserialize, Serialize};

use crate::{
    OpsError, PackFormat,
    pack::write::{BUFFER, Feed, Member},
    queue::{Backend, Queue, one_backend},
};

/// How an archive update runs; the defaults are those of the pack dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    /// 0 stores, 9 packs hardest: the new entries, and a 7z block encoded again.
    pub level: u8,
    /// 7z: the new files in one compressed block instead of one per file.
    pub solid: bool,
    /// Folders are added with everything inside.
    pub recurse: bool,
    /// Names keep their folders, relative to the sources' directory.
    pub keep_paths: bool,
    /// Remove the added files once the archive is replaced ("move to archive", F6).
    pub delete_sources: bool,
    /// Store links as links (tar, zip); off adds the file a link leads to.
    pub store_links: bool,
    /// Read the new archive back before it replaces the old one: its entries counted, the new
    /// ones read through their checksums (`archive.verify_after_update`, Total Commander's
    /// `VerifyZIP=1`).
    pub verify: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            level: 6,
            solid: true,
            recurse: true,
            keep_paths: true,
            delete_sources: false,
            store_links: false,
            verify: true,
        }
    }
}

/// What lets go of an archive file before an update replaces it: the engine closes the reading
/// sessions of the archive backends open on it (Windows replaces no open file).
pub trait ArchiveRelease: Send + Sync {
    /// Lets go of `archive`, the archive file.
    fn release(&self, archive: &VfsPath);
}

/// One archive update as the job holds it.
pub(crate) struct Change<'a> {
    pub(crate) archive: &'a VfsPath,
    pub(crate) add: &'a [VfsPath],
    pub(crate) under: &'a str,
    pub(crate) remove: &'a [u64],
    pub(crate) settings: &'a UpdateSettings,
}

/// The source and target backend of an update that can work: the archive on a local drive.
pub(crate) fn check(
    queue: &Queue,
    archive: &VfsPath,
    add: &[VfsPath],
    remove: &[u64],
) -> Result<((BackendId, Backend), Backend), OpsError> {
    if archive.backend != BackendId::Local {
        return Err(OpsError::ArchiveNotLocal {
            path: archive.path.clone(),
        });
    }
    if add.is_empty() && remove.is_empty() {
        return Err(OpsError::NoSources);
    }
    let target = queue.backend(&archive.backend)?;
    let source = if add.is_empty() {
        (archive.backend.clone(), target.clone())
    } else {
        let id = one_backend(add)?;
        let backend = queue.backend(&id)?;
        (id, backend)
    };
    Ok((source, target))
}

/// The formats an update writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Zip,
    SevenZ,
    Tar,
}

impl Kind {
    /// The kind of `format`; `None` for one that is not changed (a compressed tar, a single
    /// compressed file).
    pub(crate) fn of(format: ArchiveFormat) -> Option<Self> {
        match format {
            ArchiveFormat::Zip => Some(Self::Zip),
            ArchiveFormat::SevenZ => Some(Self::SevenZ),
            ArchiveFormat::Tar { compression: None } => Some(Self::Tar),
            ArchiveFormat::Tar { .. } | ArchiveFormat::Single { .. } => None,
        }
    }

    /// The pack format whose writer adds the new entries.
    pub(crate) fn pack_format(self) -> PackFormat {
        match self {
            Self::Zip => PackFormat::Zip,
            Self::SevenZ => PackFormat::SevenZ,
            Self::Tar => PackFormat::Tar,
        }
    }
}

/// One entry of the old archive as an update sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Old {
    /// The format's index (zip, 7z) or position (tar), as the archive backend lists it.
    pub(crate) index: u64,
    /// The stored name, `/`-separated, without a leading `./` or `/` or a trailing `/`.
    pub(crate) name: String,
    pub(crate) dir: bool,
    pub(crate) size: u64,
    pub(crate) modified: Option<SystemTime>,
}

/// One rewrite of an archive, run as a blocking task.
pub(crate) struct Rewrite {
    pub(crate) kind: Kind,
    /// The archive file as it is.
    pub(crate) old: PathBuf,
    /// The temporary file, created new.
    pub(crate) temp: PathBuf,
    /// The indices of the entries left out.
    pub(crate) remove: BTreeSet<u64>,
    /// The new entries, their names inside the archive.
    pub(crate) members: Vec<Member>,
    pub(crate) level: u32,
    pub(crate) solid: bool,
    pub(crate) limits: ArchiveLimits,
    pub(crate) verify: bool,
}

/// What a rewrite did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Stats {
    /// Entries of the old archive copied as they were stored.
    pub(crate) raw: u64,
    /// 7z: the numbers in the new archive of the blocks copied without being decoded.
    pub(crate) raw_blocks: Vec<usize>,
    /// Entries of the old archive decoded to be encoded again (7z: a block that lost one).
    pub(crate) decoded: u64,
    /// Entries the new archive holds.
    pub(crate) entries: u64,
}

/// Why a rewrite refuses an archive, carried inside its I/O error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    NotModifiable,
    Encrypted,
    /// The new archive did not read back as written.
    Unverified,
}

impl fmt::Display for Refused {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotModifiable => "this archive cannot be changed",
            Self::Encrypted => "the archive holds encrypted entries",
            Self::Unverified => "the new archive does not read back as written",
        })
    }
}

impl std::error::Error for Refused {}

pub(crate) fn refused(why: Refused) -> io::Error {
    io::Error::other(why)
}

/// The refusal `error` carries, if any.
pub(crate) fn refusal(error: &io::Error) -> Option<Refused> {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<Refused>())
        .copied()
}

/// The error of an update of `archive` that failed with `error`.
pub(crate) fn ops_error(error: io::Error, archive: &Path) -> OpsError {
    let path = archive.to_path_buf();
    match refusal(&error) {
        Some(Refused::NotModifiable) => return OpsError::ArchiveNotModifiable { path },
        Some(Refused::Encrypted) => return OpsError::ArchiveEncrypted { path },
        Some(Refused::Unverified) => return OpsError::VerifyFailed { path },
        None => {}
    }
    if crate::pack::write::is_panic(&error) {
        return OpsError::PackFailed { path };
    }
    let error = OpsError::from_io(error, archive);
    // An encrypted entry of another archive to add: never written, no password asked.
    if matches!(error.code(), "password_required" | "password_wrong") {
        return OpsError::ArchiveEncrypted { path };
    }
    error
}

/// The entries of the archive `path` of `kind`, refusing what is not changed.
pub(crate) fn scan(kind: Kind, path: &Path, limits: &ArchiveLimits) -> io::Result<Vec<Old>> {
    match kind {
        Kind::Zip => zip::scan(path),
        Kind::SevenZ => sevenz::scan(path, limits),
        Kind::Tar => tar::scan(path),
    }
}

/// Writes the new archive into `job.temp`, flushed to disk, and reads it back when `verify`.
pub(crate) fn rewrite(job: &Rewrite, feed: &Feed) -> io::Result<Stats> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&job.temp)?;
    let out = BufWriter::with_capacity(BUFFER, file);
    let (file, stats) = match job.kind {
        Kind::Zip => zip::rewrite(job, feed, out)?,
        Kind::SevenZ => sevenz::rewrite(job, feed, out)?,
        Kind::Tar => tar::rewrite(job, feed, out)?,
    };
    file.sync_all()?;
    drop(file);
    if job.verify {
        match job.kind {
            Kind::Zip => zip::verify(job, &stats)?,
            Kind::SevenZ => sevenz::verify(job, &stats)?,
            Kind::Tar => tar::verify(job, &stats)?,
        }
    }
    Ok(stats)
}

/// A stored name as the update compares it: `/`-separated, without a leading `./` or `/` and
/// without a trailing `/`.
pub(crate) fn normalize(name: &str) -> String {
    let name = name.replace('\\', "/");
    let mut name = name.as_str();
    loop {
        if let Some(rest) = name.strip_prefix("./") {
            name = rest;
        } else if let Some(rest) = name.strip_prefix('/') {
            name = rest;
        } else {
            break;
        }
    }
    name.trim_end_matches('/').to_owned()
}

/// The prefix the new entries get for the folder `under` inside the archive: `a/b/`, empty at
/// the root. Each part must be a plain name.
pub(crate) fn prefix(under: &str) -> Result<String, OpsError> {
    let under = normalize(under);
    if under.is_empty() {
        return Ok(String::new());
    }
    for part in under.split('/') {
        crate::check_name(part)?;
    }
    Ok(format!("{under}/"))
}

/// The room the update needs on the archive's drive: the new archive beside the old one, at most
/// the old one and every new file whole, and a MiB for headers.
pub(crate) fn needed_space(archive: u64, added: u64) -> u64 {
    archive.saturating_add(added).saturating_add(1 << 20)
}

/// A Windows FILETIME (100 ns since 1601) as a time; `None` for 0, which means unset.
pub(crate) fn file_time(ticks: u64) -> Option<SystemTime> {
    const TICKS_PER_SECOND: u64 = 10_000_000;
    const SECONDS_1601_TO_1970: u64 = 11_644_473_600;
    if ticks == 0 {
        return None;
    }
    // Before 1970 counts as unknown: an update only shows it in a conflict question.
    let seconds = (ticks / TICKS_PER_SECOND).checked_sub(SECONDS_1601_TO_1970)?;
    let nanos = u32::try_from((ticks % TICKS_PER_SECOND) * 100).ok()?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::new(seconds, nanos))
}

/// A reader of the old archive that waits while the job is paused and stops once it is
/// cancelled, before each read.
pub(crate) struct Paced<'a, R> {
    inner: R,
    feed: &'a Feed,
}

impl<'a, R> Paced<'a, R> {
    pub(crate) fn new(inner: R, feed: &'a Feed) -> Self {
        Self { inner, feed }
    }
}

impl<R: Read> Read for Paced<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.feed.checkpoint()?;
        self.inner.read(buf)
    }
}

impl<R: Seek> Seek for Paced<'_, R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}

/// The archive file, opened buffered for reading.
pub(crate) fn open_old(path: &Path) -> io::Result<io::BufReader<File>> {
    Ok(io::BufReader::with_capacity(BUFFER, File::open(path)?))
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
