//! One archive's reads: whether an entry can be read at all, and the producer loop of its format
//! that a reading session runs ([`super::session`]).

use std::path::Path;

use super::{
    ArchiveFormat, ArchiveLimits, ArchivePassword,
    session::{Feed, Request},
    sevenz_format, single,
    source::Source,
    tar_format,
    tree::Node,
    zip_format,
};
use crate::{EntryKind, Result, VfsError};

/// What a producer reads the archive with.
pub(crate) struct Reading {
    pub source: Source,
    pub format: ArchiveFormat,
    pub limits: ArchiveLimits,
    /// The password a person gave for the archive, if any.
    pub password: Option<ArchivePassword>,
}

impl Reading {
    /// Serves `first` and the requests after it until the producer ends; its archive reads stop
    /// once the reader they work for is gone.
    pub(crate) fn serve(self, first: Request, feed: &mut Feed) {
        let Self {
            source,
            format,
            limits,
            password,
        } = self;
        let source = source.watched(feed.watch());
        // A zip reads only an encrypted entry with the password; a 7z every entry, since one
        // with an encrypted header needs it for everything it lists.
        let password = password.as_ref();
        match format {
            ArchiveFormat::Zip => zip_format::serve(&source, password, first, feed),
            ArchiveFormat::SevenZ => sevenz_format::serve(&source, &limits, password, first, feed),
            ArchiveFormat::Tar { compression } => {
                tar_format::serve(&source, compression, limits, first, feed);
            }
            ArchiveFormat::Single { compression } => {
                single::serve(&source, compression, limits, first, feed);
            }
        }
    }
}

/// The format's index and the size of the file at `path`, or why it cannot be read; an
/// encrypted entry needs `with_password`.
pub(crate) fn readable(
    node: &Node,
    path: &Path,
    limits: &ArchiveLimits,
    with_password: bool,
) -> Result<(u64, u64)> {
    let refuse = |operation| VfsError::Unsupported {
        path: path.to_path_buf(),
        operation,
    };
    match &node.entry.kind {
        EntryKind::File => {}
        EntryKind::Dir => {
            return Err(VfsError::IsADirectory {
                path: path.to_path_buf(),
            });
        }
        EntryKind::Symlink { .. } => {
            return Err(refuse("following a link inside an archive"));
        }
        EntryKind::Other => return Err(refuse("reading this kind of archive entry")),
    }
    if node.info.encrypted && !with_password {
        return Err(VfsError::PasswordRequired {
            path: path.to_path_buf(),
        });
    }
    limits.check_entry(node.entry.size, node.info.packed_size, path)?;
    let index = node.index.ok_or_else(|| VfsError::NotFound {
        path: path.to_path_buf(),
    })?;
    Ok((index, node.entry.size))
}
