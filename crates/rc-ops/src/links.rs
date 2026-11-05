//! Links to folders in a copy or a move (RC-080-63), settled as Total Commander's `CopyLinks`
//! and `IgnoreLinks` settle them, by [`CopyLinks`] (`ops.copy_links`):
//!
//! - A move within one file system renames the link itself, whatever the setting says; its
//!   target and the target's contents stay where they are.
//! - `contents` (the default, TC's `CopyLinks=0`): the plan walks into the folder the link leads
//!   to, and the link arrives as a plain folder with that folder's contents. A move copies them
//!   and then removes only the link at the source, never the folder it leads to or anything in
//!   it - and the link only when everything below it arrived. A link that leads back into its
//!   own way (a folder above it, or one the walk came through by another link) is not followed:
//!   `link_loop`, so a copy never runs without end; the real paths are worked out from the
//!   links' stored targets, and no more than [`MAX_LINK_DEPTH`] links are followed one inside
//!   another.
//! - `link` (TC's `CopyLinks=1`): the link is made anew at the target with its target unchanged
//!   (a relative one stays relative), as a symbolic link, or a junction where the target makes
//!   no symbolic link without a privilege; a target that makes neither is `links_unsupported`.
//! - `skip` (TC's `IgnoreLinks=1`): the link is passed over and counted as skipped.
//!
//! A link to a file is copied as its contents, whatever the setting says, as TC does.

use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use rc_vfs::{Entry, EntryKind, LinkKind, TargetKind, VfsError};
use serde::{Deserialize, Serialize};

use crate::{
    OpsError,
    copy::falls_back,
    runner::{Runner, Step},
    walk::Item,
};

/// What a copy or a move does with a link to a folder it does not rename.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyLinks {
    /// The folder it leads to is copied as a plain folder.
    #[default]
    Contents,
    /// The link is made anew at the target, leading where it led.
    Link,
    /// The link is passed over, counted as skipped.
    Skip,
}

impl CopyLinks {
    /// Whether this is the default, [`CopyLinks::Contents`].
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The value of the setting `ops.copy_links`; anything it does not name is the default.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value {
            "link" => Self::Link,
            "skip" => Self::Skip,
            _ => Self::Contents,
        }
    }
}

/// The most links to folders a copy follows one inside another; deeper ones are `link_loop`.
/// The bound of Linux's own link resolution.
pub(crate) const MAX_LINK_DEPTH: usize = 40;

/// Whether `entry` is a link whose target is a folder.
pub(crate) fn is_dir_link(entry: &Entry) -> bool {
    matches!(
        entry.kind,
        EntryKind::Symlink {
            target_kind: Some(TargetKind::Dir),
            ..
        }
    )
}

/// How a planned item stands to the links the plan walked into.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Mark {
    /// A link to a folder, planned as the folder: its contents follow it in the plan.
    pub(crate) link: bool,
    /// Below such a link: copied only, never renamed or removed at the source.
    pub(crate) below: bool,
}

/// `path` with every `.` dropped and every `..` taken back, without asking the file system.
fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The way the walk took to an item: where it really is once links were followed, and the
/// folders it stood in when it followed each of them.
#[derive(Debug, Clone, Default)]
pub(crate) struct Trail {
    /// The item's path with every link on the way replaced by its target; `None` while the walk
    /// followed none.
    real: Option<PathBuf>,
    /// The folder each followed link lay in, outermost first.
    jumps: Arc<Vec<PathBuf>>,
}

impl Trail {
    /// Whether the item lies below a followed link.
    pub(crate) fn below(&self) -> bool {
        self.real.is_some()
    }

    /// The trail of the entry `name` inside the folder this trail leads to.
    pub(crate) fn child(&self, name: &str) -> Self {
        Self {
            real: self.real.as_ref().map(|real| real.join(name)),
            jumps: Arc::clone(&self.jumps),
        }
    }

    /// The trail into the folder of the link at `path` (this trail's item), which stores
    /// `target`; `None` when the folder lies on the way here - one the walk stands in, or one
    /// above it - or the links go too deep. A `source` link is one of the job's sources: the
    /// folder it lies in is not copied, so it is no part of the way.
    pub(crate) fn follow(&self, path: &Path, target: &Path, source: bool) -> Option<Self> {
        if self.jumps.len() >= MAX_LINK_DEPTH {
            return None;
        }
        let real = self.real.as_deref().unwrap_or(path);
        let parent = real.parent();
        // A relative target is relative to the link's folder, as the file system reads it.
        let into = normal(&match parent {
            Some(folder) if target.is_relative() => folder.join(target),
            _ => target.to_path_buf(),
        });
        let on_way = |folder: &Path| folder.starts_with(&into);
        if self.jumps.iter().any(|folder| on_way(folder)) || (!source && parent.is_some_and(on_way))
        {
            return None;
        }
        let mut jumps = self.jumps.as_ref().clone();
        if let Some(folder) = parent.filter(|_| !source) {
            jumps.push(folder.to_path_buf());
        }
        Some(Self {
            real: Some(into),
            jumps: Arc::new(jumps),
        })
    }
}

/// How a link to a folder the plan did not walk into was handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkDone {
    /// Renamed to the target: the move of the link itself.
    Moved,
    /// Made anew at the target; a move still owes the source's removal.
    Made,
    /// Passed over (`skip`).
    Skipped,
}

/// The kind a link is made anew as on a target that makes `links`: a symbolic link, or a
/// junction to an absolute folder where no symbolic link is made; `None` for a target that
/// makes neither.
fn link_kind(links: rc_vfs::LinkKinds, target: &Path) -> Option<LinkKind> {
    if links.symlink {
        Some(LinkKind::Symlink)
    } else if links.junction && target.is_absolute() {
        Some(LinkKind::Junction)
    } else {
        None
    }
}

impl Runner {
    /// A link to a folder the plan did not walk into, to `to` on the target: within one file
    /// system a move renames it; else `copy_links` decides - `link` makes it anew, `skip`
    /// passes it over, and with `contents`, which walks every other one, it is a link on its
    /// own way (`link_loop`).
    pub(crate) async fn dir_link(
        &mut self,
        item: &Item,
        to: &Path,
        moving: bool,
    ) -> Result<LinkDone, Step> {
        if moving && self.rename_ok {
            match self.src.rename_no_replace(&item.path, to).await {
                Ok(()) => return Ok(LinkDone::Moved),
                Err(error) if falls_back(&error) => self.rename_ok = false,
                Err(error) => return Err(error.into()),
            }
        }
        let EntryKind::Symlink { target, .. } = &item.entry.kind else {
            // The caller passes links only.
            return Ok(LinkDone::Skipped);
        };
        match self.options.copy_links {
            crate::CopyLinks::Skip => Ok(LinkDone::Skipped),
            crate::CopyLinks::Contents => Err(OpsError::LinkLoop {
                path: item.path.clone(),
            }
            .into()),
            crate::CopyLinks::Link => {
                let links = self.dst.property_capabilities().links;
                let unsupported = || OpsError::LinksUnsupported {
                    path: to.to_path_buf(),
                };
                let kind = link_kind(links, target).ok_or_else(unsupported)?;
                match self.dst.create_link(to, target, kind).await {
                    Ok(()) => Ok(LinkDone::Made),
                    // Windows without the privilege for symbolic links: a junction leads to a
                    // local folder as well.
                    Err(VfsError::SymlinkPrivilege { .. })
                        if links.junction && target.is_absolute() =>
                    {
                        self.dst.create_link(to, target, LinkKind::Junction).await?;
                        Ok(LinkDone::Made)
                    }
                    Err(VfsError::Unsupported { .. }) => Err(unsupported().into()),
                    Err(error) => Err(error.into()),
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
