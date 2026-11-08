//! Where rCommander keeps its data (RC-100-95): one folder, decided once at the start of the
//! process and read by everything that lives in it - the database ([`crate::DB_FILE`]), its
//! backups ([`crate::BACKUP_DIR`]), a staged restore, the MCP instance file, the installed
//! plugins, the update's journal and copies, zsh's shell integration.
//!
//! The user's folder is the platform's (`%APPDATA%\degoya\rCommander\data`,
//! `~/.local/share/rcommander`); a portable copy keeps `data/` beside the program instead. The
//! program decides which (`rcommander`'s `portable.rs`) and hands it over with [`set_location`]
//! before anything opens; until then, and in every process that never sets it (the tests, a
//! tool), it is the user's folder.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::{BACKUP_DIR, DB_FILE, DbError, Result};

/// The data folder and how it was chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataLocation {
    /// The platform's data folder in the person's profile.
    User(PathBuf),
    /// `data/` beside a portable program, switched on by its marker file.
    Portable(PathBuf),
}

impl DataLocation {
    /// The platform's data folder.
    ///
    /// # Errors
    ///
    /// [`DbError::NoDataDir`] when the platform reports no home directory.
    pub fn user() -> Result<Self> {
        project_dirs()
            .map(|dirs| Self::User(dirs.data_dir().to_path_buf()))
            .ok_or(DbError::NoDataDir)
    }

    /// The folder itself.
    #[must_use]
    pub fn dir(&self) -> &Path {
        match self {
            Self::User(dir) | Self::Portable(dir) => dir,
        }
    }

    /// Whether this is a portable copy's folder.
    #[must_use]
    pub fn is_portable(&self) -> bool {
        matches!(self, Self::Portable(_))
    }

    /// `rcommander.db` in it.
    #[must_use]
    pub fn database(&self) -> PathBuf {
        self.dir().join(DB_FILE)
    }

    /// The backups' folder in it.
    #[must_use]
    pub fn backups(&self) -> PathBuf {
        self.dir().join(BACKUP_DIR)
    }

    /// The folder for what belongs to this machine (zsh's shell integration): the portable
    /// folder itself, else the platform's local data folder (`%LOCALAPPDATA%` on Windows).
    #[must_use]
    pub fn local_dir(&self) -> Option<PathBuf> {
        match self {
            Self::Portable(dir) => Some(dir.clone()),
            Self::User(_) => project_dirs().map(|dirs| dirs.data_local_dir().to_path_buf()),
        }
    }
}

/// A location that is set once and never changes after.
#[derive(Debug, Default)]
pub struct LocationCell(OnceLock<DataLocation>);

impl LocationCell {
    /// An empty cell.
    #[must_use]
    pub const fn new() -> Self {
        Self(OnceLock::new())
    }

    /// Sets the location; a second call changes nothing and hands its location back.
    ///
    /// # Errors
    ///
    /// The location given, when one was set before.
    pub fn set(&self, location: DataLocation) -> std::result::Result<(), DataLocation> {
        self.0.set(location)
    }

    /// The location set, if one was.
    #[must_use]
    pub fn get(&self) -> Option<&DataLocation> {
        self.0.get()
    }

    /// The location set, else the user's folder.
    ///
    /// # Errors
    ///
    /// [`DbError::NoDataDir`] when nothing was set and the platform reports no home directory.
    pub fn resolve(&self) -> Result<DataLocation> {
        match self.get() {
            Some(location) => Ok(location.clone()),
            None => DataLocation::user(),
        }
    }
}

/// The process's location.
static LOCATION: LocationCell = LocationCell::new();

/// Sets the process's data folder, once, before anything opens the database.
///
/// # Errors
///
/// The location given, when one was set before; the first one stays.
pub fn set_location(location: DataLocation) -> std::result::Result<(), DataLocation> {
    LOCATION.set(location)
}

/// The process's data folder and how it was chosen: the one [`set_location`] set, else the
/// user's.
///
/// # Errors
///
/// [`DbError::NoDataDir`] when nothing was set and the platform reports no home directory.
pub fn location() -> Result<DataLocation> {
    LOCATION.resolve()
}

/// The process's data folder.
///
/// # Errors
///
/// [`DbError::NoDataDir`] when nothing was set and the platform reports no home directory.
pub fn data_dir() -> Result<PathBuf> {
    location().map(|location| location.dir().to_path_buf())
}

/// Whether the process runs as a portable copy.
#[must_use]
pub fn is_portable() -> bool {
    LOCATION.get().is_some_and(DataLocation::is_portable)
}

fn project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("de", "degoya", "rCommander")
}

#[cfg(test)]
#[path = "location_tests.rs"]
mod tests;
