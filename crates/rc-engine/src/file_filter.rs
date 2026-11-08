//! Total Commander's file filters (RC-100-92): Ctrl+F10 shows all files, Ctrl+F11 only programs,
//! Ctrl+F12 the files a mask matches. A filter applies to files; folders always show. It stays
//! when the panel moves to another folder and belongs to the tab (or, with
//! `tabs.filter_per_tab` off, to the panel), so it is kept with the tabs.
//!
//! Which extensions are programs is the setting `filters.programs` (`exe com bat cmd pif`, as
//! Total Commander's help lists them); off Windows a file with an execute bit is one too. The
//! extensions in force are this process's ([`set_programs`]), as the name order is
//! (`sort::set_names`): the engine sets them from the setting at the start and on each change.

use std::sync::{Arc, Mutex, OnceLock};

use rc_vfs::Entry;
use serde::{Deserialize, Serialize};

use crate::{EngineError, extension, mask::Mask, recover::LockOrRecover as _};

/// The setting that names the programs' extensions.
pub const PROGRAMS_KEY: &str = "filters.programs";

/// Its default: Total Commander's programs.
pub const DEFAULT_PROGRAMS: &str = "exe com bat cmd pif";

/// Whether each tab keeps its own file filter (Total Commander's `DirTabFilters`); off, the tabs
/// of a panel share one.
pub const FILTER_PER_TAB_KEY: &str = "tabs.filter_per_tab";

/// The longest mask a file filter takes, in characters.
pub const MAX_FILTER_MASK: usize = 1024;

/// What a panel's file filter shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "mask", rename_all = "snake_case")]
pub enum FileFilter {
    /// Every file (Ctrl+F10).
    #[default]
    All,
    /// Programs only (Ctrl+F11).
    Programs,
    /// The files a mask line matches (Ctrl+F12): `*.txt;*.md|read*`, as Num+ takes it.
    Mask(String),
}

impl FileFilter {
    /// Whether every file shows.
    #[must_use]
    pub fn is_all(&self) -> bool {
        *self == Self::All
    }

    /// The filter as far as it can be used: a blank mask is no filter; a mask longer than
    /// [`MAX_FILTER_MASK`] characters or holding a control character is refused.
    ///
    /// # Errors
    ///
    /// [`EngineError::FilterInvalid`].
    pub fn checked(self) -> Result<Self, EngineError> {
        let Self::Mask(mask) = self else {
            return Ok(self);
        };
        let mask = mask.trim();
        if mask.is_empty() {
            return Ok(Self::All);
        }
        if mask.chars().count() > MAX_FILTER_MASK {
            return Err(EngineError::FilterInvalid {
                reason: "longer than 1024 characters",
            });
        }
        if mask.chars().any(char::is_control) {
            return Err(EngineError::FilterInvalid {
                reason: "control characters",
            });
        }
        Ok(Self::Mask(mask.to_owned()))
    }

    /// What decides entry by entry; `None` while every file shows.
    pub(crate) fn matcher(&self) -> Option<FileMatcher> {
        match self {
            Self::All => None,
            Self::Programs => Some(FileMatcher::Programs {
                extensions: programs(),
                execute_bit: !cfg!(windows),
            }),
            Self::Mask(mask) => Some(FileMatcher::Mask(Mask::parse(mask))),
        }
    }

    /// The filter as stored with a tab: JSON, `"null"` for all files.
    pub(crate) fn stored(&self) -> String {
        if self.is_all() {
            return "null".to_owned();
        }
        serde_json::to_string(self).unwrap_or_else(|_| "null".to_owned())
    }

    /// A stored filter read back as untrusted: one that does not read or does not pass
    /// [`FileFilter::checked`] is no filter.
    pub(crate) fn from_stored(text: &str) -> Self {
        serde_json::from_str::<Option<Self>>(text)
            .ok()
            .flatten()
            .and_then(|filter| filter.checked().ok())
            .unwrap_or_default()
    }
}

/// A file filter ready to judge entries.
#[derive(Debug, Clone)]
pub(crate) enum FileMatcher {
    /// The extensions of programs, lower case; with `execute_bit` a file with one counts too.
    Programs {
        extensions: Arc<[String]>,
        execute_bit: bool,
    },
    Mask(Mask),
}

impl FileMatcher {
    /// Whether `entry` shows: every folder, and the files the filter takes.
    pub(crate) fn shows(&self, entry: &Entry) -> bool {
        if entry.is_dir_like() {
            return true;
        }
        match self {
            Self::Programs {
                extensions,
                execute_bit,
            } => is_program(entry, extensions, *execute_bit),
            // In a flat view the name carries its folders: the mask judges the file's own name.
            Self::Mask(mask) => mask.matches(own_name(&entry.name), false),
        }
    }
}

/// The last part of a name that may carry folders (a flat view's `sub\file.txt`).
fn own_name(name: &str) -> &str {
    name.rsplit(['\\', '/']).next().unwrap_or(name)
}

/// Whether `entry` is a program: its extension is one of `extensions` (lower case), or - with
/// `execute_bit` - it has an execute bit.
pub(crate) fn is_program(entry: &Entry, extensions: &[String], execute_bit: bool) -> bool {
    let ext = extension(entry).to_lowercase();
    (!ext.is_empty() && extensions.contains(&ext))
        || (execute_bit && entry.attributes.mode.is_some_and(|mode| mode & 0o111 != 0))
}

fn programs_cell() -> &'static Mutex<Arc<[String]>> {
    static PROGRAMS: OnceLock<Mutex<Arc<[String]>>> = OnceLock::new();
    PROGRAMS.get_or_init(|| Mutex::new(split_extensions(DEFAULT_PROGRAMS)))
}

/// The programs' extensions in force.
fn programs() -> Arc<[String]> {
    let programs = programs_cell().lock_or_recover("file_filter.programs");
    Arc::clone(&*programs)
}

/// The extensions of `list` (`exe com`, as the setting stores it) are programs from now on, for
/// the whole process.
pub fn set_programs(list: &str) {
    *programs_cell().lock_or_recover("file_filter.programs") = split_extensions(list);
}

fn split_extensions(list: &str) -> Arc<[String]> {
    list.split_whitespace()
        .map(|word| word.trim_start_matches('.').to_lowercase())
        .filter(|word| !word.is_empty())
        .collect()
}

#[cfg(test)]
#[path = "file_filter_tests.rs"]
mod tests;
