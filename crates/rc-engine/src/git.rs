//! The git status of a panel's folder (RC-100-97, owner 2026-11-07): a letter beside each
//! changed file, a dot on a folder with changes below it, ignored entries dimmed, the branch in
//! the panel's status line. rCommander's own feature - Total Commander has none.
//!
//! Nothing starts `git` unasked: `panels.git_status` is off by default and only the person
//! switches it on (`local`, `local_network`). A panel that shows a folder anew - navigated, read
//! again with F2, after an operation, on the window's return - asks for its marks after a short
//! pause (`follow.rs`). rCommander finds the repository itself by looking for `.git` from the
//! folder up (`discover.rs`, `stat` calls only): outside a repository no process starts; servers,
//! archives and devices never run one. `run.rs` runs the system's `git` (or a WSL distribution's
//! own, only while that distribution runs) with a time limit, an output limit and a hardening
//! against the repository's configuration: no fsmonitor, every filter driver emptied, no
//! optional locks. `porcelain.rs` reads its answer, `marks.rs` turns the paths into marks of the
//! folder's entries. `panel.git_status` reports what a panel knows and starts nothing.

mod discover;
mod follow;
mod marks;
mod porcelain;
mod run;

use std::collections::BTreeMap;

use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

pub(crate) use follow::GitDesk;
pub use porcelain::{Change, Item, PorcelainError, Status, parse as parse_porcelain};
pub use run::{MIN_VERSION, Version};

/// What git says of an entry, from the least to the most important: a folder's dot takes the
/// colour of its most important child (conflict, modified, added, renamed, untracked).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitMark {
    /// Ignored by `.gitignore`: drawn dimmed, never a folder's dot.
    Ignored,
    /// Not tracked yet (`U`).
    Untracked,
    /// Renamed in the index (`R`).
    Renamed,
    /// Added to the index, or copied (`A`).
    Added,
    /// Changed in the index or in the folder (`M`).
    Modified,
    /// Unmerged: a conflict to resolve (`C`).
    Conflict,
}

/// The branch the repository stands on, as `git status --branch` reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitBranch {
    /// The branch's name; `None` when HEAD is detached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// The commit HEAD names; `None` before the first commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oid: Option<String>,
    /// The branch it follows, when it follows one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    /// Commits the upstream lacks.
    #[serde(default)]
    pub ahead: u32,
    /// Commits of the upstream it lacks.
    #[serde(default)]
    pub behind: u32,
}

/// A folder's git marks, as a panel showing it keeps them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitView {
    /// The folder the marks are for.
    pub folder: VfsPath,
    /// The branch, unless `panels.git_branch` is off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<GitBranch>,
    /// Each entry's mark by name (in lower case where names ignore case): a file's own, a
    /// folder's own when git names it whole, else the most important of what lies below it.
    pub marks: BTreeMap<String, GitMark>,
    /// The mark of every entry when git names the folder itself whole (an untracked or an
    /// ignored folder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whole: Option<GitMark>,
    /// Files deleted from the folder: not listed, only counted.
    #[serde(default)]
    pub deleted: usize,
    /// Names compared without their case (Windows and macOS).
    #[serde(skip)]
    pub(crate) fold_case: bool,
}

impl GitView {
    /// The mark of the entry named `name`.
    #[must_use]
    pub fn mark(&self, name: &str) -> Option<GitMark> {
        let own = if self.fold_case {
            self.marks.get(&name.to_lowercase())
        } else {
            self.marks.get(name)
        };
        own.copied().or(self.whole)
    }
}

/// Where `panels.git_status` lets git run (owner 2026-11-07: off by default).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GitScope {
    /// Nowhere.
    #[default]
    Off,
    /// Fixed and removable drives and WSL distributions.
    Local,
    /// Network drives and shares too.
    LocalNetwork,
}

impl GitScope {
    /// The scope a setting's value names; off for anything else.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value {
            "local" => Self::Local,
            "local_network" => Self::LocalNetwork,
            _ => Self::Off,
        }
    }
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
