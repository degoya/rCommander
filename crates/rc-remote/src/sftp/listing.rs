//! Listings and `stat`: the server's names and attributes as [`Entry`] values.
//!
//! A directory is read in the blocks the server answers `READDIR` with, every block counted
//! against [`super::SftpLimits::max_entries`] and the whole listing against
//! [`super::SftpLimits::listing_timeout`]. Each block is passed on as it arrives
//! ([`rc_vfs::Vfs::list_batches`], RC-060-02), so a panel shows the first entries of a long
//! listing before its end; a receiver that is gone ends the listing after the block in flight. A
//! name that is not one entry of the directory ([`safe_name`]) is dropped. Links are listed as
//! links: their target is read and followed a few at a time, so a link to a directory can be
//! entered and shows its target's size and time. The owner comes from the `longname`
//! (`owner.rs`).

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use futures_util::{StreamExt, stream};
use rc_vfs::{Attributes, Entry, EntryKind, Result, TargetKind, VfsError, unix_time};
use russh_sftp::protocol::{FileAttributes, FileType};
use tokio::{
    sync::mpsc::{self, Sender},
    time::timeout,
};

use super::{
    SftpError, SftpVfs,
    error::{is_eof, is_no_such_file},
    owner,
    path::wire_path,
};
pub(crate) use crate::listed_name::safe_name;
use crate::wire::child;

/// How many links of one listing are resolved at once.
const LINKS_AT_ONCE: usize = 16;
/// Blocks read ahead of a receiver that is slower than the server.
const BLOCKS_AHEAD: usize = 4;

/// A link's target as stored, and the target's attributes when it exists.
type LinkTarget = (PathBuf, Option<FileAttributes>);

/// The kind of an entry by its attributes; a server that sends no type bits lists files.
fn kind_of(attrs: &FileAttributes) -> FileType {
    match attrs.permissions {
        None => FileType::File,
        Some(_) => attrs.file_type(),
    }
}

fn time_of(attrs: &FileAttributes) -> Option<SystemTime> {
    attrs.mtime.and_then(|secs| unix_time(u64::from(secs)))
}

/// The entry `name` with `attrs`; a link without its target (see [`link_entry`]).
pub(crate) fn entry(name: String, attrs: &FileAttributes) -> Entry {
    let kind = match kind_of(attrs) {
        FileType::Dir => EntryKind::Dir,
        FileType::File => EntryKind::File,
        FileType::Symlink => EntryKind::Symlink {
            target: PathBuf::new(),
            target_kind: None,
        },
        FileType::Other => EntryKind::Other,
    };
    let mode = attrs.permissions.map(|bits| bits & 0o7777);
    Entry {
        hidden: name.starts_with('.'),
        owner: owner::of_attrs(attrs),
        name,
        size: if kind == EntryKind::File {
            attrs.size.unwrap_or(0)
        } else {
            0
        },
        kind,
        modified: time_of(attrs),
        created: None,
        attributes: mode.map(Attributes::from_mode).unwrap_or_default(),
    }
}

/// `link` with its `target` as stored and what the target is (`None`: dangling); size and time
/// are the target's when it exists, as a file manager shows a link.
pub(crate) fn link_entry(
    mut link: Entry,
    target: PathBuf,
    resolved: Option<&FileAttributes>,
) -> Entry {
    let target_kind = resolved.map(|attrs| match kind_of(attrs) {
        FileType::Dir => TargetKind::Dir,
        FileType::File => TargetKind::File,
        FileType::Symlink | FileType::Other => TargetKind::Other,
    });
    if let Some(attrs) = resolved {
        link.size = match target_kind {
            Some(TargetKind::File) => attrs.size.unwrap_or(0),
            _ => 0,
        };
        link.modified = time_of(attrs).or(link.modified);
    }
    link.kind = EntryKind::Symlink {
        target,
        target_kind,
    };
    link
}

impl SftpVfs {
    /// The entries of `dir`; links resolved when `links`, else listed without their target
    /// (enough to remove a tree, which never follows one).
    pub(super) async fn list_dir(&self, dir: &Path, links: bool) -> Result<Vec<Entry>> {
        let (sender, mut receiver) = mpsc::channel(BLOCKS_AHEAD);
        let mut entries = Vec::new();
        let collect = async {
            while let Some(block) = receiver.recv().await {
                entries.extend(block);
            }
        };
        let (listed, ()) = tokio::join!(self.list_blocks(dir, links, sender), collect);
        listed.map(|()| entries)
    }

    /// The entries of `dir` sent to `blocks` one `READDIR` answer at a time, links resolved
    /// when `links`; the listing ends early, without an error, once the receiver is gone.
    pub(super) async fn list_blocks(
        &self,
        dir: &Path,
        links: bool,
        blocks: Sender<Vec<Entry>>,
    ) -> Result<()> {
        let wire = wire_path(dir)?;
        let limits = self.limits;
        timeout(
            limits.listing_timeout,
            self.read_dir(dir, &wire, links, blocks),
        )
        .await
        .map_err(|_| {
            SftpError::Timeout {
                seconds: limits.listing_timeout.as_secs(),
            }
            .into_vfs(dir)
        })?
    }

    /// Every block of the listing of `dir`, the handle closed again whatever happened.
    async fn read_dir(
        &self,
        dir: &Path,
        wire: &str,
        links: bool,
        sender: Sender<Vec<Entry>>,
    ) -> Result<()> {
        let handle = match self.unless_gone(self.raw.opendir(wire)).await {
            Ok(handle) => handle.handle,
            Err(error) => return Err(self.explain_list(dir, error).await),
        };
        let max = self.limits.max_entries;
        let (mut listed, mut blocks, mut dropped) = (0usize, 0usize, 0usize);
        let outcome = loop {
            match self.unless_gone(self.raw.readdir(handle.as_str())).await {
                Ok(block) => {
                    // A block without names still counts: a server cannot keep a listing alive
                    // with empty answers.
                    blocks += 1;
                    listed = listed.saturating_add(block.files.len());
                    if listed > max || blocks > max {
                        break Err(SftpError::ListingTooLarge { limit: max }.into_vfs(dir));
                    }
                    let mut entries = Vec::with_capacity(block.files.len());
                    for file in block.files {
                        if matches!(file.filename.as_str(), "." | "..") {
                            continue;
                        }
                        if !safe_name(&file.filename) {
                            dropped += 1;
                            continue;
                        }
                        let mut found = entry(file.filename, &file.attrs);
                        if let Some(named) = owner::of_longname(&file.longname) {
                            found.owner = Some(named);
                        }
                        entries.push(found);
                    }
                    if links {
                        self.resolve_links(wire, &mut entries).await;
                    }
                    // Nobody listens any more: the rest is not read.
                    if !entries.is_empty() && sender.send(entries).await.is_err() {
                        break Ok(());
                    }
                }
                Err(error) if is_eof(&error) => break Ok(()),
                Err(error) => break Err(self.fail(error, dir)),
            }
        };
        if !self.is_closed() {
            // The listing stands without the handle closed; a failure here changes nothing.
            let _ = self.raw.close(handle).await;
        }
        if dropped > 0 {
            tracing::warn!(
                dropped,
                "an SFTP listing named entries outside its directory"
            );
        }
        outcome
    }

    /// Why opening the directory `dir` failed: the server says "no such file" also for a path
    /// that is a file.
    async fn explain_list(&self, dir: &Path, error: russh_sftp::client::error::Error) -> VfsError {
        if is_no_such_file(&error)
            && let Ok(Some(attrs)) = self.probe(dir).await
            && !attrs.is_dir()
        {
            return VfsError::NotADirectory {
                path: dir.to_path_buf(),
            };
        }
        self.fail(error, dir)
    }

    /// Reads target and kind of every link in `entries`, [`LINKS_AT_ONCE`] at a time. A link
    /// whose target cannot be read stays a link without one.
    async fn resolve_links(&self, wire_dir: &str, entries: &mut [Entry]) {
        let links: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches!(entry.kind, EntryKind::Symlink { .. }))
            .map(|(index, _)| index)
            .collect();
        let resolved: Vec<(usize, Option<LinkTarget>)> = stream::iter(links)
            .map(|index| {
                let wire = child(wire_dir, &entries[index].name);
                async move { (index, self.target_of(wire).await) }
            })
            .buffered(LINKS_AT_ONCE)
            .collect()
            .await;
        for (index, found) in resolved {
            if let Some((target, attrs)) = found {
                entries[index] = link_entry(entries[index].clone(), target, attrs.as_ref());
            }
        }
    }

    /// A link's stored target and the attributes of what it leads to (`None`: dangling).
    async fn target_of(&self, wire: String) -> Option<LinkTarget> {
        let target = self.raw.readlink(wire.as_str()).await.ok()?;
        let target = target.files.into_iter().next()?.filename;
        if target.contains('\0') {
            return None;
        }
        let attrs = self.raw.stat(wire).await.ok().map(|attrs| attrs.attrs);
        Some((PathBuf::from(target), attrs))
    }

    /// The entry at `path`, a link described as the link.
    pub(super) async fn stat_path(&self, path: &Path) -> Result<Entry> {
        let wire = wire_path(path)?;
        let attrs = self
            .raw
            .lstat(wire.as_str())
            .await
            .map_err(|error| self.fail(error, path))?
            .attrs;
        let name = path.file_name().map_or_else(
            || "/".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let found = entry(name, &attrs);
        if !matches!(found.kind, EntryKind::Symlink { .. }) {
            return Ok(found);
        }
        Ok(match self.target_of(wire).await {
            Some((target, resolved)) => link_entry(found, target, resolved.as_ref()),
            None => found,
        })
    }
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
