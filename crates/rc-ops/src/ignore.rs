//! The ignore list of the panels (RC-100-92) as copies and moves see it: Total Commander's
//! ignored entries "will not be copied (with F5), even if in subfolders, will not be moved (with
//! F6), except they are in a folder moved on the same drive, will not be counted".
//!
//! The engine hands the queue its list ([`crate::Queue::set_ignore`]) while it is switched on. A
//! copy's or a move's walk leaves out every entry below a source that the list hides - it is
//! neither copied, nor moved, nor counted - and a moved folder that still holds one stays where
//! it was (`copy_dirs.rs` leaves a source folder that is not empty). A folder moved whole by one
//! rename takes them along, as Total Commander's does. Delete, pack and the rest walk everything:
//! what they find is the engine's to show (the delete question counts it).

use std::path::Path;

use rc_vfs::{Entry, VfsPath};

/// Which entries the panels never show.
pub trait IgnoreRule: Send + Sync {
    /// Whether `entry`, listed in the folder `dir`, is hidden.
    fn ignores(&self, dir: &VfsPath, entry: &Entry) -> bool;
}

/// A rule over the paths of one backend, for a walk that knows paths only ([`crate::dir_size_without`]).
pub type SkipEntry<'a> = &'a (dyn Fn(&Path, &Entry) -> bool + Send + Sync);
