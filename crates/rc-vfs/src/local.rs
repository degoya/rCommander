//! The local backend: the machine's own drives, network shares and - on Windows - WSL
//! distributions, which are UNC shares (`\\wsl.localhost\<distro>\...`).
//!
//! Every path goes through [`long_path`] before it reaches the file system, so Windows does not
//! stop at 260 characters; errors carry the path as the caller gave it. Listing and `stat` run
//! as one blocking task each: a directory of 100 000 entries is one task, not one per entry.

mod describe;
mod links;
#[cfg(any(windows, test))]
mod reparse;
pub(crate) mod space;
#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use unix as platform;
#[cfg(windows)]
use windows as platform;

use std::{
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use async_trait::async_trait;
use tokio::io::AsyncSeekExt;

use crate::{
    Capabilities, Details, Entry, LinkKind, PropertyCapabilities, PropertyChange, ReadStream,
    Result, SeekStream, SetAttributes, Vfs, VfsError, WriteMode, WriteStream, long_path,
};

/// The local file system.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalFs;

impl LocalFs {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// Runs blocking file-system work off the async threads.
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

/// Maps an I/O error to the caller's spelling of the path.
fn io_err(path: &Path) -> impl FnOnce(io::Error) -> VfsError + '_ {
    move |error| VfsError::from_io(error, path)
}

/// Opens a file for reading; a directory is `IsADirectory`.
async fn open_file(path: &Path) -> Result<tokio::fs::File> {
    let file = tokio::fs::File::open(long_path(path))
        .await
        .map_err(io_err(path))?;
    let meta = file.metadata().await.map_err(io_err(path))?;
    if meta.is_dir() {
        return Err(VfsError::IsADirectory {
            path: path.to_path_buf(),
        });
    }
    Ok(file)
}

#[async_trait]
impl Vfs for LocalFs {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            rename_across_dirs: true,
            symlinks: true,
            case_sensitive: cfg!(not(any(windows, target_os = "macos"))),
            unix_mode: cfg!(unix),
            set_mtime: true,
            free_space: true,
        }
    }

    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        let shown = dir.to_path_buf();
        blocking(dir, move || describe::list(&shown)).await
    }

    async fn stat(&self, path: &Path) -> Result<Entry> {
        let shown = path.to_path_buf();
        blocking(path, move || describe::stat(&shown)).await
    }

    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        Ok(Box::new(open_file(path).await?))
    }

    async fn open_read_at(&self, path: &Path, offset: u64) -> Result<ReadStream> {
        let mut file = open_file(path).await?;
        if offset > 0 {
            file.seek(io::SeekFrom::Start(offset))
                .await
                .map_err(io_err(path))?;
        }
        Ok(Box::new(file))
    }

    async fn open_read_seekable(&self, path: &Path) -> Result<SeekStream> {
        Ok(Box::new(open_file(path).await?))
    }

    async fn open_write(&self, path: &Path, mode: WriteMode) -> Result<WriteStream> {
        let mut options = tokio::fs::OpenOptions::new();
        match mode {
            WriteMode::CreateNew => options.write(true).create_new(true),
            WriteMode::Truncate => options.write(true).create(true).truncate(true),
            WriteMode::Append => options.append(true).create(true),
        };
        let file = options.open(long_path(path)).await.map_err(io_err(path))?;
        Ok(Box::new(file))
    }

    async fn mkdir(&self, path: &Path, recursive: bool) -> Result<()> {
        let real = long_path(path);
        let made = if recursive {
            tokio::fs::create_dir_all(real).await
        } else {
            tokio::fs::create_dir(real).await
        };
        made.map_err(io_err(path))
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        tokio::fs::rename(long_path(from), long_path(to))
            .await
            .map_err(io_err(from))
    }

    /// One call that refuses an existing `to` (Linux `renameat2` with `RENAME_NOREPLACE`,
    /// Windows `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`); a file system without it
    /// (drvfs, 9p, an older kernel) gets the look first.
    async fn rename_no_replace(&self, from: &Path, to: &Path) -> Result<()> {
        let (real_from, real_to) = (long_path(from), long_path(to));
        let renamed = blocking(from, move || {
            Ok(platform::rename_no_replace(&real_from, &real_to))
        })
        .await?;
        match renamed {
            Some(Ok(())) => Ok(()),
            Some(Err(error)) if error.kind() == io::ErrorKind::AlreadyExists => {
                Err(VfsError::AlreadyExists {
                    path: to.to_path_buf(),
                })
            }
            Some(Err(error)) => Err(VfsError::from_io(error, from)),
            None => crate::vfs::rename_after_look(self, from, to).await,
        }
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        let shown = path.to_path_buf();
        blocking(path, move || {
            platform::remove_file(&long_path(&shown)).map_err(io_err(&shown))
        })
        .await
    }

    async fn remove_dir(&self, path: &Path, recursive: bool) -> Result<()> {
        let real = long_path(path);
        let removed = if recursive {
            tokio::fs::remove_dir_all(real).await
        } else {
            tokio::fs::remove_dir(real).await
        };
        removed.map_err(io_err(path))
    }

    async fn set_mtime(&self, path: &Path, modified: SystemTime) -> Result<()> {
        let shown = path.to_path_buf();
        blocking(path, move || {
            filetime::set_file_mtime(
                long_path(&shown),
                filetime::FileTime::from_system_time(modified),
            )
            .map_err(io_err(&shown))
        })
        .await
    }

    async fn set_attributes(&self, path: &Path, change: SetAttributes) -> Result<()> {
        let shown: PathBuf = path.to_path_buf();
        blocking(path, move || platform::set_attributes(&shown, change)).await
    }

    async fn free_space(&self, path: &Path) -> Result<Option<u64>> {
        let shown = path.to_path_buf();
        blocking(path, move || {
            space::disk_space(&long_path(&shown))
                .map(|(available, _total)| Some(available))
                .map_err(io_err(&shown))
        })
        .await
    }

    fn property_capabilities(&self) -> PropertyCapabilities {
        platform::CAPABILITIES
    }

    async fn details(&self, path: &Path) -> Result<Details> {
        let shown = path.to_path_buf();
        blocking(path, move || describe::details(&shown)).await
    }

    async fn list_details(&self, dir: &Path) -> Result<Vec<(Entry, Details)>> {
        let shown = dir.to_path_buf();
        blocking(dir, move || describe::list_details(&shown)).await
    }

    async fn set_properties(&self, path: &Path, change: PropertyChange) -> Result<()> {
        let shown = path.to_path_buf();
        blocking(path, move || platform::set_properties(&shown, change)).await
    }

    async fn create_link(&self, link: &Path, target: &Path, kind: LinkKind) -> Result<()> {
        let (shown, target) = (link.to_path_buf(), target.to_path_buf());
        blocking(link, move || links::create_link(&shown, &target, kind)).await
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
