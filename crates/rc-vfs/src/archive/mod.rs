//! The archive backend: the inside of a zip, 7z or tar file (plain or compressed with gzip, xz,
//! bzip2 or zstd) and of a single compressed file, read-only, as a [`Vfs`] of its own.
//!
//! The archive file lies on any other backend and is read through it - zip and 7z with
//! [`Vfs::open_read_seekable`], tar and single files as a stream. The archive crates are
//! synchronous, so every scan and every read is one blocking task that drives the outer
//! backend ([`source`]); a read pumps the entry through a bounded channel ([`pipe`]), so even an
//! entry deep in a solid 7z block streams instead of unpacking into memory. The opened archive
//! stays with a reading session between reads ([`session`]): reads in the archive's order walk it
//! once.
//!
//! The listing is scanned once and cached until the archive file's size or time changes.
//! Encrypted zip and 7z entries are read with a password a person typed ([`ArchivePassword`],
//! [`ArchiveFs::with_password`]); without one they are `password_required`.
//! Writing is `unsupported`: packing is a way of its own (RC-030-04). Entry names are never
//! joined to a real path; names that climb out are cleaned and marked, links are shown and
//! never followed, and [`ArchiveLimits`] stop archive bombs ([`safety`]).

mod decode;
mod detect;
mod password;
mod pipe;
mod read;
mod safety;
mod session;
mod sevenz_format;
mod sevenz_memory;
mod single;
mod source;
mod tar_format;
mod tree;
mod zip_format;

use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::SystemTime,
};

use async_trait::async_trait;
use encoding_rs::Encoding;
use serde::{Deserialize, Serialize};

pub use password::ArchivePassword;
pub use safety::{ArchiveLimits, UnsafeName};
pub use tree::{ArchiveEntryInfo, ArchiveItem};

use self::{
    read::{Reading, readable},
    session::{Feed, Request, Serve, Sessions, Wanted},
    single::Outer,
    source::Source,
    tree::{RawEntry, Tree},
};
use crate::{
    Capabilities, Entry, ReadStream, Result, SetAttributes, Vfs, VfsError, WriteMode, WriteStream,
};

/// What kind of archive a file is, from its first bytes ([`detect_format`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    /// A tar file, plain or inside a compressed stream.
    Tar {
        compression: Option<Compression>,
    },
    /// One compressed file, shown as an archive with one entry.
    Single {
        compression: Compression,
    },
}

/// A compressed stream around a tar or a single file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    Gzip,
    Xz,
    Bzip2,
    Zstd,
}

/// The settings of the archive backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArchiveSettings {
    /// The limits against archive bombs.
    pub limits: ArchiveLimits,
    /// The code page of zip names stored without the UTF-8 flag: `cp437`, what the zip
    /// specification says and DOS and Windows write, or any WHATWG encoding label (`ibm866`,
    /// `shift_jis`, `gbk`, `windows-1252`, ...). An unknown label falls back to CP437.
    pub zip_legacy_encoding: String,
}

impl Default for ArchiveSettings {
    fn default() -> Self {
        Self {
            limits: ArchiveLimits::default(),
            zip_legacy_encoding: "cp437".to_owned(),
        }
    }
}

impl ArchiveSettings {
    /// The encoding of legacy zip names; `None` is CP437, which the zip reader decodes itself.
    pub(crate) fn legacy_encoding(&self) -> Option<&'static Encoding> {
        let label = self.zip_legacy_encoding.trim();
        if ["cp437", "ibm437", "437"]
            .iter()
            .any(|cp437| label.eq_ignore_ascii_case(cp437))
        {
            return None;
        }
        Encoding::for_label(label.as_bytes())
    }
}

/// The format of the file at `path` on `vfs`, from its first bytes; `None` when it is no
/// archive this backend reads. An xz stream is looked into within the default limits.
///
/// # Errors
///
/// The file cannot be read.
pub async fn detect_format(vfs: &Arc<dyn Vfs>, path: &Path) -> Result<Option<ArchiveFormat>> {
    detect_format_within(vfs, path, ArchiveLimits::default()).await
}

/// [`detect_format`] within `limits`: an xz stream whose dictionary passes their decoder memory
/// is `archive_decoder_memory`.
///
/// # Errors
///
/// The file cannot be read, or its xz header asks for too much memory.
pub async fn detect_format_within(
    vfs: &Arc<dyn Vfs>,
    path: &Path,
    limits: ArchiveLimits,
) -> Result<Option<ArchiveFormat>> {
    let file = Source::new(Arc::clone(vfs), path)?;
    blocking(path, move || {
        let stream = file.stream()?;
        detect::sniff(stream, &limits).map_err(|error| source::io_error(error, file.path()))
    })
    .await
}

/// One archive, read-only, as a file system: `/` is the archive's root.
pub struct ArchiveFs {
    outer: Arc<dyn Vfs>,
    archive: PathBuf,
    format: ArchiveFormat,
    settings: ArchiveSettings,
    /// The password for encrypted entries, when a person gave one (RC-030-03).
    password: Option<ArchivePassword>,
    cache: Mutex<Option<Cached>>,
    /// The producer that keeps the archive open between reads (RC-080-46).
    sessions: Sessions,
}

/// The tree of the archive file as it was when scanned.
struct Cached {
    stamp: Stamp,
    tree: Arc<Tree>,
}

/// What tells that the archive file changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: Option<SystemTime>,
}

impl ArchiveFs {
    /// The archive `archive` on `outer`, of a known format.
    #[must_use]
    pub fn new(
        outer: Arc<dyn Vfs>,
        archive: impl Into<PathBuf>,
        format: ArchiveFormat,
        settings: ArchiveSettings,
    ) -> Self {
        Self {
            outer,
            archive: archive.into(),
            format,
            settings,
            password: None,
            cache: Mutex::new(None),
            sessions: Sessions::default(),
        }
    }

    /// The same archive, its encrypted entries (and a 7z's encrypted header) read with
    /// `password`. A wrong one makes those reads `password_wrong`.
    #[must_use]
    pub fn with_password(mut self, password: ArchivePassword) -> Self {
        self.password = Some(password);
        self.cache = Mutex::new(None);
        self.sessions = Sessions::default();
        self
    }

    /// The archive `archive` on `outer`, its format detected; `None` when it is no archive.
    ///
    /// # Errors
    ///
    /// The file cannot be read.
    pub async fn open(
        outer: Arc<dyn Vfs>,
        archive: impl Into<PathBuf>,
        settings: ArchiveSettings,
    ) -> Result<Option<Self>> {
        let archive = archive.into();
        let format = detect_format_within(&outer, &archive, settings.limits).await?;
        Ok(format.map(|format| Self::new(outer, archive, format, settings)))
    }

    /// The format read.
    #[must_use]
    pub fn format(&self) -> ArchiveFormat {
        self.format
    }

    /// The entries of `dir` with what the archive says about each (encrypted, unsafe name).
    ///
    /// # Errors
    ///
    /// As [`Vfs::list`].
    pub async fn list_detailed(&self, dir: &Path) -> Result<Vec<ArchiveItem>> {
        let items = self.list_ordered(dir).await?;
        Ok(items.into_iter().map(|(item, _)| item).collect())
    }

    /// [`Self::list_detailed`] with where each entry lies in the archive - its index (zip, 7z) or
    /// position (tar); `None` for a folder the archive does not state. Entries read in this
    /// order walk the archive once: a solid 7z block or a compressed tar unpacks once, not once
    /// per entry.
    ///
    /// # Errors
    ///
    /// As [`Vfs::list`].
    pub async fn list_ordered(&self, dir: &Path) -> Result<Vec<(ArchiveItem, Option<u64>)>> {
        let tree = self.tree().await?;
        let node = tree.find(dir)?;
        if !node.is_dir() {
            return Err(VfsError::NotADirectory {
                path: dir.to_path_buf(),
            });
        }
        Ok(tree
            .children(node)
            .map(|child| (child.item(), child.index))
            .collect())
    }

    /// The entry at `path` with what the archive says about it.
    ///
    /// # Errors
    ///
    /// As [`Vfs::stat`].
    pub async fn stat_detailed(&self, path: &Path) -> Result<ArchiveItem> {
        Ok(self.tree().await?.find(path)?.item())
    }

    /// The tree, scanned again when the archive file changed since the last scan.
    async fn tree(&self) -> Result<Arc<Tree>> {
        Ok(self.current().await?.0)
    }

    /// The tree and the state of the archive file it was scanned from.
    async fn current(&self) -> Result<(Arc<Tree>, Stamp)> {
        let outer = self.outer.stat(&self.archive).await?;
        if outer.is_dir_like() {
            return Err(VfsError::IsADirectory {
                path: self.archive.clone(),
            });
        }
        let stamp = Stamp {
            size: outer.size,
            modified: outer.modified,
        };
        let cached = self
            .slot()
            .as_ref()
            .filter(|cached| cached.stamp == stamp)
            .map(|cached| Arc::clone(&cached.tree));
        if let Some(tree) = cached {
            return Ok((tree, stamp));
        }
        let scan = Scan {
            source: Source::new(Arc::clone(&self.outer), &self.archive)?,
            format: self.format,
            settings: self.settings.clone(),
            password: self.password.clone(),
            outer: Outer {
                name: outer.name,
                size: outer.size,
                modified: outer.modified,
            },
        };
        let tree = Arc::new(blocking(&self.archive, move || scan.run().map(Tree::build)).await?);
        *self.slot() = Some(Cached {
            stamp,
            tree: Arc::clone(&tree),
        });
        Ok((tree, stamp))
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Option<Cached>> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A listing scan, run as a blocking task.
struct Scan {
    source: Source,
    format: ArchiveFormat,
    settings: ArchiveSettings,
    password: Option<ArchivePassword>,
    outer: Outer,
}

impl Scan {
    fn run(self) -> Result<Vec<RawEntry>> {
        let Self {
            source,
            format,
            settings,
            password,
            outer,
        } = self;
        match format {
            ArchiveFormat::Zip => zip_format::scan(&source, &settings),
            ArchiveFormat::SevenZ => sevenz_format::scan(&source, &settings, password.as_ref()),
            ArchiveFormat::Tar { compression } => tar_format::scan(&source, compression, &settings),
            ArchiveFormat::Single { compression } => {
                single::scan(&source, compression, &settings, &outer)
            }
        }
    }
}

/// Runs blocking work off the async threads.
async fn blocking<T, F>(path: &Path, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|join| VfsError::Io {
            path: path.to_path_buf(),
            source: io::Error::other(join),
        })?
}

fn read_only(path: &Path) -> VfsError {
    VfsError::Unsupported {
        path: path.to_path_buf(),
        operation: "changing an archive",
    }
}

#[async_trait]
impl Vfs for ArchiveFs {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            rename_across_dirs: false,
            symlinks: true,
            case_sensitive: true,
            unix_mode: false,
            set_mtime: false,
            free_space: false,
        }
    }

    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        let items = self.list_detailed(dir).await?;
        Ok(items.into_iter().map(|item| item.entry).collect())
    }

    async fn stat(&self, path: &Path) -> Result<Entry> {
        Ok(self.stat_detailed(path).await?.entry)
    }

    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        let (tree, stamp) = self.current().await?;
        let node = tree.find(path)?;
        let (index, size) = readable(node, path, &self.settings.limits, self.password.is_some())?;
        let wanted = Wanted {
            index,
            size,
            path: path.to_path_buf(),
            encrypted: node.info.encrypted,
        };
        let reading = Reading {
            source: Source::new(Arc::clone(&self.outer), &self.archive)?,
            format: self.format,
            limits: self.settings.limits,
            password: self.password.clone(),
        };
        let serve: Serve =
            Box::new(move |first: Request, feed: &mut Feed| reading.serve(first, feed));
        self.sessions.open(stamp, wanted, serve).await
    }

    async fn open_write(&self, path: &Path, _mode: WriteMode) -> Result<WriteStream> {
        Err(read_only(path))
    }

    async fn mkdir(&self, path: &Path, _recursive: bool) -> Result<()> {
        Err(read_only(path))
    }

    async fn rename(&self, from: &Path, _to: &Path) -> Result<()> {
        Err(read_only(from))
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        Err(read_only(path))
    }

    async fn remove_dir(&self, path: &Path, _recursive: bool) -> Result<()> {
        Err(read_only(path))
    }

    async fn set_mtime(&self, path: &Path, _modified: SystemTime) -> Result<()> {
        Err(read_only(path))
    }

    async fn set_attributes(&self, path: &Path, _change: SetAttributes) -> Result<()> {
        Err(read_only(path))
    }
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
