//! Packing (RC-030-04): files and folders into a new zip, 7z or tar archive, as a job.
//!
//! The sources are read through their backend's [`rc_vfs::Vfs`] like any copy; the archive is a
//! local file - the archive writers are synchronous and a 7z is written with seeks - so a pack
//! onto another backend is refused when submitted (pack locally, then copy). The writer runs as one
//! blocking task (`write.rs`) that pulls each source through the runtime and reports its bytes
//! back; pause and cancel act between two buffers there as they do in a copy.
//!
//! A panic in an archive crate (ruzstd's encoder unwraps I/O errors) never ends the app: the
//! writer runs on a blocking task, so the panic fails that archive with `pack_failed` and its
//! temporary file is removed like after any failure.
//!
//! The archive is written under a temporary name beside the target and renamed onto it only when
//! complete, so a cancelled or failed pack leaves nothing under the target's name - and an
//! overwritten archive stays whole until the new one replaces it. With `delete_sources` the
//! packed files are removed only after their archive is in place (Total Commander's "move to
//! archive"); the dialog and the MCP confirmation code ask before.
//!
//! Formats: zip (deflate, levels 0 to 9, zip64 where a file needs it, UTF-8 names), 7z (LZMA2 with
//! the level as its preset, 0 stores; solid or one block per file), tar plain or compressed with
//! gzip, xz, bzip2 or zstd. The zstd encoder is ruzstd's, pure Rust, which knows two levels:
//! 0 stores, every other level is its fastest (about zstd's level 1). Times and Unix permissions
//! go into the archive where the format holds them; links are stored as links (tar, zip) or
//! packed as the file they lead to, as chosen.

mod members;
mod sevenz;
mod tar;
mod write;
mod zip;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_vfs::{EntryKind, VfsError, VfsPath, long_path};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use self::write::{Feed, Member, Spec};
use crate::{
    Failure, JobState, OpsError,
    runner::{Runner, Step, Stop, attempt},
    walk::Item,
};

/// The archive format a pack writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackFormat {
    #[default]
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "7z")]
    SevenZ,
    #[serde(rename = "tar")]
    Tar,
    #[serde(rename = "tar_gz")]
    TarGz,
    #[serde(rename = "tar_xz")]
    TarXz,
    #[serde(rename = "tar_bz2")]
    TarBz2,
    #[serde(rename = "tar_zst")]
    TarZst,
}

impl PackFormat {
    /// Every format, in the order the dialog offers them.
    pub const ALL: [Self; 7] = [
        Self::Zip,
        Self::SevenZ,
        Self::Tar,
        Self::TarGz,
        Self::TarXz,
        Self::TarBz2,
        Self::TarZst,
    ];

    /// The file ending of an archive of this format, without the dot.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZ => "7z",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
            Self::TarXz => "tar.xz",
            Self::TarBz2 => "tar.bz2",
            Self::TarZst => "tar.zst",
        }
    }

    /// Whether the format stores links as links.
    fn stores_links(self) -> bool {
        self != Self::SevenZ
    }
}

/// How a pack runs; the defaults are Total Commander's pack dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PackSettings {
    pub format: PackFormat,
    /// 0 stores, 9 packs hardest.
    pub level: u8,
    /// 7z: one compressed block for all files (smaller) instead of one per file.
    pub solid: bool,
    /// Folders are packed with everything inside; off packs only the files named and the
    /// folders themselves, empty.
    pub recurse: bool,
    /// Names in the archive keep their folders, relative to the sources' directory; off stores
    /// bare file names.
    pub keep_paths: bool,
    /// Each source into an archive of its own, named after it, in the directory `target`.
    pub each_own_archive: bool,
    /// Remove the packed files once their archive is complete ("move to archive").
    pub delete_sources: bool,
    /// Store links as links (tar, zip); off packs the file a link leads to.
    pub store_links: bool,
    /// Replace an archive that exists; off fails it with `already_exists`.
    pub overwrite: bool,
}

impl Default for PackSettings {
    fn default() -> Self {
        Self {
            format: PackFormat::Zip,
            level: 6,
            solid: true,
            recurse: true,
            keep_paths: true,
            each_own_archive: false,
            delete_sources: false,
            store_links: false,
            overwrite: false,
        }
    }
}

/// One archive to write: where, and the items that go in.
struct Group {
    dest: PathBuf,
    items: Vec<Item>,
}

/// The source an item belongs to: the first part of its path below the sources' directory.
fn top(item: &Item) -> Option<&std::ffi::OsStr> {
    item.rel.components().next().map(|part| part.as_os_str())
}

/// The archive of its own a source named `name` gets in `dir`. A listed name is untrusted: one
/// that is no plain file name, with the format's ending, is refused (RC-080-43).
fn own_archive(dir: &Path, name: &str, ending: &str) -> Result<PathBuf, OpsError> {
    let name = format!("{name}.{ending}");
    crate::check_name(&name)?;
    Ok(dir.join(name))
}

impl Runner {
    /// Packs `sources` into `target` (an archive, or a directory with `each_own_archive`).
    pub(crate) async fn pack(
        &mut self,
        sources: &[VfsPath],
        target: &VfsPath,
        settings: &PackSettings,
    ) -> Result<(), Stop> {
        let dir = if settings.each_own_archive {
            Some(target.path.as_path())
        } else {
            target.path.parent()
        };
        if let Some(dir) = dir {
            let dst = Arc::clone(&self.dst);
            if attempt!(
                self,
                target,
                crate::copy::ensure_dir(dst.as_ref(), dir).await
            )
            .is_none()
            {
                return Ok(());
            }
        }
        let items = if settings.recurse {
            self.plan(sources).await?
        } else {
            self.plan_flat(sources).await?
        };
        // An MCP job packs no file its read guard refuses now (RC-080-42).
        let items = self.admitted(items).await?;
        let groups = if settings.each_own_archive {
            let ending = settings.format.extension();
            let mut groups: Vec<Group> = Vec::new();
            // A source whose name is refused, with what it holds: not packed, counted as done.
            let mut refused: Option<PathBuf> = None;
            for item in items {
                // A source and what it holds lie together in the plan.
                if refused
                    .as_deref()
                    .is_some_and(|rel| item.rel.starts_with(rel))
                {
                    if item.is_file() {
                        self.count_done(1, item.entry.size);
                    }
                    continue;
                }
                match groups.last_mut() {
                    Some(group) if top(&group.items[0]) == top(&item) => group.items.push(item),
                    _ => {
                        let at = self.src_path(&item.path);
                        let named = || own_archive(&target.path, &item.entry.name, ending);
                        match attempt!(self, &at, named().map_err(Step::from)) {
                            Some(dest) => groups.push(Group {
                                dest,
                                items: vec![item],
                            }),
                            None => {
                                if item.is_file() {
                                    self.count_done(1, item.entry.size);
                                }
                                refused = Some(item.rel);
                            }
                        }
                    }
                }
            }
            groups
        } else {
            vec![Group {
                dest: target.path.clone(),
                items,
            }]
        };
        let mut packed: Vec<Item> = Vec::new();
        for group in groups {
            self.checkpoint().await?;
            let at = VfsPath {
                backend: target.backend.clone(),
                path: group.dest.clone(),
            };
            let files: Vec<&Item> = group.items.iter().filter(|item| item.is_file()).collect();
            let (count, bytes) = (
                files.len() as u64,
                files.iter().map(|item| item.entry.size).sum::<u64>(),
            );
            let start = self.progress.bytes_done;
            let written = attempt!(self, &at, {
                self.progress.restart_file(start);
                self.write_group(&group, settings).await
            });
            self.progress.bytes_done = start;
            self.count_done(count, bytes);
            if let Some(members) = written {
                packed.extend(members);
            }
        }
        if settings.delete_sources {
            self.delete_packed(&packed).await?;
        }
        Ok(())
    }

    /// The sources themselves, without what folders hold.
    async fn plan_flat(&mut self, sources: &[VfsPath]) -> Result<Vec<Item>, Stop> {
        let mut items = Vec::new();
        for source in sources {
            self.checkpoint().await?;
            let src = Arc::clone(&self.src);
            let Some(entry) = attempt!(
                self,
                source,
                src.stat(&source.path).await.map_err(Step::from)
            ) else {
                continue;
            };
            if entry.kind != EntryKind::Dir {
                self.progress.files_total += 1;
                self.progress.bytes_total += entry.size;
            }
            let rel = source
                .path
                .file_name()
                .map_or_else(|| PathBuf::from(&entry.name), PathBuf::from);
            items.push(Item {
                path: source.path.clone(),
                rel,
                entry,
            });
        }
        self.job
            .update(|snapshot| snapshot.state = JobState::Running);
        self.tick(true);
        Ok(items)
    }

    /// Writes one archive under a temporary name and renames it onto `group.dest`; the items
    /// packed.
    async fn write_group(
        &mut self,
        group: &Group,
        settings: &PackSettings,
    ) -> Result<Vec<Item>, Step> {
        let dest = long_path(&group.dest);
        match tokio::fs::symlink_metadata(&dest).await {
            Ok(meta) if meta.is_dir() => {
                return Err(OpsError::KindMismatch {
                    path: group.dest.clone(),
                }
                .into());
            }
            Ok(_) if !settings.overwrite => {
                return Err(OpsError::Vfs(VfsError::AlreadyExists {
                    path: group.dest.clone(),
                })
                .into());
            }
            _ => {}
        }
        let (members, items): (Vec<Member>, Vec<Item>) =
            self.members(group, settings).into_iter().unzip();
        let temp = crate::temp_name::pack_path(&dest, self.job.id);
        let (bytes, mut seen) = watch::channel(0_u64);
        let feed = Feed::new(Arc::clone(&self.src), self.job.control.clone(), bytes)
            .map_err(|error| OpsError::from_io(error, &group.dest))?;
        let current = feed.current();
        let grown = feed.grown();
        let spec = Spec {
            members,
            format: settings.format,
            level: settings.level.min(9),
            solid: settings.solid,
            temp: temp.clone(),
        };
        let start = self.progress.bytes_done;
        let mut task = tokio::task::spawn_blocking(move || write::write(&spec, &feed));
        let joined = loop {
            tokio::select! {
                joined = &mut task => break joined,
                changed = seen.changed() => {
                    if changed.is_err() {
                        break (&mut task).await;
                    }
                    let read = *seen.borrow_and_update();
                    self.progress.bytes_done = start + read;
                    if let Some(member) = current.get() {
                        let at = self.src_path(&member.path);
                        self.progress.begin_file(at, member.size);
                        self.progress.file_bytes_done = read.saturating_sub(member.from);
                    }
                    self.tick(false);
                }
            }
        };
        let written = match joined {
            Ok(written) => written,
            // A panic in an archive crate fails this archive; the app goes on.
            Err(_) => Err(write::panicked()),
        };
        let renamed = match written {
            Ok(()) => tokio::fs::rename(&temp, &dest).await,
            Err(error) => Err(error),
        };
        if let Err(error) = renamed {
            if let Err(left) = tokio::fs::remove_file(&temp).await
                && left.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(
                    path = %temp.display(),
                    "an unfinished archive could not be removed: {left}"
                );
            }
            if self.job.control.is_cancelled() {
                return Err(Step::Stop(Stop::Cancelled));
            }
            if write::is_panic(&error) {
                return Err(OpsError::PackFailed {
                    path: group.dest.clone(),
                }
                .into());
            }
            return Err(OpsError::from_io(error, &group.dest).into());
        }
        Ok(self.without_grown(items, &grown.take()))
    }

    /// "Move to archive": the packed files go, then each folder of theirs that is left empty.
    async fn delete_packed(&mut self, packed: &[Item]) -> Result<(), Stop> {
        let src = Arc::clone(&self.src);
        for item in packed.iter().filter(|item| item.is_file()) {
            self.checkpoint().await?;
            let at = self.src_path(&item.path);
            let _removed = attempt!(
                self,
                &at,
                src.remove_file(&item.path).await.map_err(Step::from)
            );
        }
        let mut dirs: Vec<&Item> = packed.iter().filter(|item| !item.is_file()).collect();
        dirs.sort_by_key(|item| std::cmp::Reverse(item.path.components().count()));
        for dir in dirs {
            match src.remove_dir(&dir.path, false).await {
                Ok(()) | Err(VfsError::DirectoryNotEmpty { .. } | VfsError::NotFound { .. }) => {}
                Err(error) => {
                    let failure = Failure::new(&self.src_path(&dir.path), &error.into());
                    self.job.update(|snapshot| snapshot.failures.push(failure));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
