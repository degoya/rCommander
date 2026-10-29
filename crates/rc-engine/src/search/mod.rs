//! The file search (Total Commander's Alt+F7): a recursive walk over any [`rc_vfs::Vfs`] that
//! reports the entries matching a [`SearchQuery`] as they are found.
//!
//! The query combines a name pattern (masks with exclusions, or a regular expression,
//! [`mask`]), conditions on size, date, attributes and kind ([`Filters`]) and optionally a text
//! inside the files ([`ContentQuery`], streamed in bounded chunks). The walk ([`walk`]) lists
//! one directory at a time, files of a directory before its subdirectories, each by name, so
//! two runs over an unchanged tree report the same hits in the same order - what the paging of
//! `files.search` relies on when its walk was not kept ([`pages`]). Symbolic links are not followed unless asked, and then with a
//! cycle guard. A search stops at the next entry or chunk once its [`SearchCancel`] is set.

mod content;
mod filters;
mod mask;
pub(crate) mod pages;
mod walk;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use rc_vfs::{Entry, VfsPath};
use serde::{Deserialize, Serialize};

pub use content::{ContentQuery, TextEncoding};
pub use filters::{Filters, KindFilter};
pub(crate) use walk::Walk;

use crate::EngineError;

/// What to search for. Every field is optional in its serde form; the default finds every
/// entry, hidden ones included, without following links.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchQuery {
    /// Masks as Total Commander takes them (`*.rs;*.toml | target\`), or a regular expression
    /// when `name_regex` is set; empty matches every name.
    pub name: String,
    pub name_regex: bool,
    /// Whether case counts in `name`; it does not by default.
    pub case_sensitive: bool,
    /// A text the files have to contain (or not contain).
    pub content: Option<ContentQuery>,
    pub filters: Filters,
    /// How many directory levels below a start folder are searched; `Some(0)` is the start
    /// folder only, `None` has no limit.
    pub max_depth: Option<u32>,
    /// Enter links to directories (each real directory once, never a link back up).
    pub follow_symlinks: bool,
    /// Leave hidden entries out, and do not enter hidden directories.
    pub skip_hidden: bool,
}

impl SearchQuery {
    /// The query with its patterns compiled.
    ///
    /// # Errors
    ///
    /// [`EngineError::InvalidSearch`] for a regular expression that does not compile or an
    /// empty content text.
    pub(crate) fn compile(&self) -> Result<Compiled, EngineError> {
        Ok(Compiled {
            names: mask::NameMatcher::new(&self.name, self.name_regex, self.case_sensitive)?,
            content: self
                .content
                .as_ref()
                .map(content::ContentMatcher::new)
                .transpose()?,
            filters: self.filters.clone(),
            max_depth: self.max_depth,
            follow_symlinks: self.follow_symlinks,
            skip_hidden: self.skip_hidden,
        })
    }
}

/// A [`SearchQuery`] ready to run.
#[derive(Debug)]
pub(crate) struct Compiled {
    names: mask::NameMatcher,
    content: Option<content::ContentMatcher>,
    filters: Filters,
    max_depth: Option<u32>,
    follow_symlinks: bool,
    skip_hidden: bool,
}

/// Stops a running search; clones share the flag.
#[derive(Debug, Clone, Default)]
pub struct SearchCancel(Arc<AtomicBool>);

impl SearchCancel {
    /// The search stops at its next entry or chunk and reports itself finished as cancelled.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// One entry the search found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Where it is, as the walk reached it (through a link when links are followed).
    pub path: VfsPath,
    pub entry: Entry,
}

/// How far a search got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchStats {
    /// Directories listed.
    pub dirs: u64,
    /// Entries looked at.
    pub entries: u64,
    pub hits: u64,
    /// Directories and files that could not be read.
    pub skipped: u64,
}

/// What a running search reports, in order; [`SearchEvent::Finished`] comes last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEvent {
    /// A match.
    Hit(SearchHit),
    /// At most every 100 ms: the directory being searched and the counts so far.
    Progress {
        dir: Option<VfsPath>,
        stats: SearchStats,
    },
    /// A directory or file that could not be read, with the code of its error; the search
    /// goes on.
    Skipped { path: VfsPath, code: &'static str },
    /// The search ended: through everything, or cancelled.
    Finished { stats: SearchStats, cancelled: bool },
}
