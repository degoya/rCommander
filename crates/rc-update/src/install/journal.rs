//! The plan the window hands the updater and the journal that records every step of it
//! (RC-100-06), written atomically before each step so a stop anywhere leaves a phase the next
//! start acts on (`recover.rs`).
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/install/mod.rs` (`Plan`, `Phase`,
//! `Journal`) at commit 26c8592a6: no service to stop, no MSI; the window ends itself, the new
//! version proves itself in the journal, and the database copy carries its BLAKE3.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    InstallError, JOURNAL_FILE, PREVIOUS_DIR, REJECTED_FILE, durable, steps::is_sha256, update_dir,
};
use crate::install_kind::InstallKind;

/// What the window hands the updater. Every path is absolute.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Plan {
    pub kind: InstallKind,
    pub from_version: String,
    pub target_version: String,
    /// The verified download.
    pub artifact: PathBuf,
    pub sha256: String,
    pub size: u64,
    /// The folder the archive's entries live in: the executable's, or the one holding the
    /// `.app` bundle on macOS.
    pub install_dir: PathBuf,
    /// The program in it, `/`-separated: `rcommander.exe`, `rcommander`, or
    /// `rCommander.app/Contents/MacOS/rcommander`.
    pub executable: String,
    pub data_dir: PathBuf,
    pub database: PathBuf,
    /// The checked copy written before the update (`rc_db` backup), put back when the new
    /// version was started and did not prove itself.
    #[serde(default)]
    pub database_copy: Option<PathBuf>,
    /// Its BLAKE3 as the backup wrote it: a roll-back puts it back only while it matches.
    #[serde(default)]
    pub database_copy_blake3: Option<String>,
    /// The window's own arguments, so the new version starts as the old one did.
    #[serde(default)]
    pub args: Vec<String>,
    /// Whether the updater starts the new version; `false` for "install when quitting"
    /// (RC-100-07): the next start a person makes proves it.
    pub start_new: bool,
    /// How long the updater waits for the new version's proof.
    pub confirm_timeout_secs: u64,
}

impl Plan {
    /// Refuses a plan the updater must not act on.
    ///
    /// # Errors
    ///
    /// `update.plan_invalid` with what is wrong.
    pub fn validate(&self) -> Result<(), InstallError> {
        let invalid = |detail: String| Err(InstallError::new("update.plan_invalid", detail));
        if self.kind != InstallKind::Portable {
            return invalid(format!("{} does not install itself", self.kind.as_str()));
        }
        for version in [&self.from_version, &self.target_version] {
            if !is_plain_version(version) {
                return invalid(format!("{version:?} is not a plain version"));
            }
        }
        if !is_sha256(&self.sha256) {
            return invalid("the artifact's digest is no SHA-256".to_owned());
        }
        if self
            .database_copy_blake3
            .as_ref()
            .is_some_and(|digest| !is_sha256(digest))
        {
            return invalid("the database copy's digest is no BLAKE3".to_owned());
        }
        for path in [
            &self.artifact,
            &self.install_dir,
            &self.data_dir,
            &self.database,
        ] {
            if !path.is_absolute() {
                return invalid(format!("{} is not absolute", path.display()));
            }
        }
        Ok(())
    }

    /// The program's first component: the entry of the archive that carries it.
    #[must_use]
    pub fn program_entry(&self) -> &str {
        self.executable.split('/').next().unwrap_or_default()
    }
}

/// Letters, digits, `.`, `_`, `+` and `-`, at most 64 of them: safe in a folder name.
#[must_use]
pub(crate) fn is_plain_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 64
        && !version.starts_with('.')
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-'))
}

/// Where an update stands.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// The window wrote the plan and started the updater; nothing has changed.
    Handed,
    /// The new files are unpacked beside the program; nothing live has changed.
    Staged,
    /// Entries are being moved.
    Switching,
    /// The new version is in place and not yet proven.
    Switched,
    /// The new version proved itself: done.
    Verified,
    /// The switch is being taken back.
    RollingBack,
    /// The previous version is back; `reason` says why.
    RolledBack,
    /// Nothing was switched, or the switch could not be taken back; `reason` says which.
    Failed,
}

impl Phase {
    /// The name in the journal and `update.status`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Handed => "handed",
            Self::Staged => "staged",
            Self::Switching => "switching",
            Self::Switched => "switched",
            Self::Verified => "verified",
            Self::RollingBack => "rolling_back",
            Self::RolledBack => "rolled_back",
            Self::Failed => "failed",
        }
    }

    /// Whether the update has ended, one way or the other.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Verified | Self::RolledBack | Self::Failed)
    }
}

/// The record of one update, from the window's hand-over to its end. Holds no secret: paths,
/// versions, digests and codes.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Journal {
    pub plan: Plan,
    pub phase: Phase,
    /// The top-level entries of the new archive, the ones the switch moves.
    #[serde(default)]
    pub entries: Vec<String>,
    /// Those of [`Self::entries`] that existed before and went to `.previous`.
    #[serde(default)]
    pub replaced: Vec<String>,
    /// Whether the new version was started, and with it may have changed the database.
    #[serde(default)]
    pub new_started: bool,
    /// Starts of the new version after the updater was gone (`recover.rs`).
    #[serde(default)]
    pub start_attempts: u32,
    /// The stable code of why it failed or was rolled back.
    #[serde(default)]
    pub reason: Option<String>,
    /// The same in words, for the log.
    #[serde(default)]
    pub detail: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Whether the leftovers of a finished update were removed.
    #[serde(default)]
    pub cleaned: bool,
}

impl Journal {
    /// A journal for `plan` in phase [`Phase::Handed`], not yet written.
    #[must_use]
    pub fn begin(plan: Plan) -> Self {
        let now = Utc::now();
        Self {
            plan,
            phase: Phase::Handed,
            entries: Vec::new(),
            replaced: Vec::new(),
            new_started: false,
            start_attempts: 0,
            reason: None,
            detail: None,
            started_at: now,
            updated_at: now,
            cleaned: false,
        }
    }

    /// `<data>/update/journal.json`.
    #[must_use]
    pub fn path(data: &Path) -> PathBuf {
        update_dir(data).join(JOURNAL_FILE)
    }

    /// The journal of the data directory, if an update was ever started from it. A journal
    /// [`Self::check`] refuses is acted on by nobody: logged, moved aside to
    /// `journal.rejected.json` untouched, and read as none.
    ///
    /// # Errors
    ///
    /// When it exists and cannot be read or parsed.
    pub fn read(data: &Path) -> Result<Option<Self>> {
        let path = Self::path(data);
        let journal: Self = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("read {}", path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        };
        if let Err(error) = journal.check(data) {
            let aside = update_dir(data).join(REJECTED_FILE);
            tracing::error!(code = error.code, detail = %error.detail, "the update journal is refused and nothing acts on it");
            durable::rename(&path, &aside)?;
            return Ok(None);
        }
        Ok(Some(journal))
    }

    /// Writes the journal where the plan's data directory keeps it, atomically.
    ///
    /// # Errors
    ///
    /// When it cannot be written; nothing may act on a step the journal does not record.
    pub fn write(&mut self) -> Result<()> {
        self.updated_at = Utc::now();
        let path = Self::path(&self.plan.data_dir);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        durable::write_atomically(&path, &serde_json::to_vec_pretty(self)?)
    }

    /// Moves to `phase` and writes.
    ///
    /// # Errors
    ///
    /// As [`Self::write`].
    pub fn advance(&mut self, phase: Phase) -> Result<()> {
        self.phase = phase;
        self.write()
    }

    /// Ends the update in `phase` with `code` and writes.
    ///
    /// # Errors
    ///
    /// As [`Self::write`].
    pub fn end(&mut self, phase: Phase, code: &str, detail: impl Into<String>) -> Result<()> {
        self.phase = phase;
        self.reason = Some(code.to_owned());
        self.detail = Some(detail.into());
        self.write()
    }

    /// `<install>/.update-<version>`: where the new files are unpacked.
    #[must_use]
    pub fn staged_dir(&self) -> PathBuf {
        self.plan
            .install_dir
            .join(format!(".update-{}", self.plan.target_version))
    }

    /// `<install>/.previous`.
    #[must_use]
    pub fn previous_dir(&self) -> PathBuf {
        self.plan.install_dir.join(PREVIOUS_DIR)
    }

    /// `<install>/.failed-<version>`: a rolled-back version's files, until they can go.
    #[must_use]
    pub fn failed_dir(&self) -> PathBuf {
        self.plan
            .install_dir
            .join(format!(".failed-{}", self.plan.target_version))
    }

    /// The program in the program folder.
    #[must_use]
    pub fn executable(&self) -> PathBuf {
        self.plan
            .executable
            .split('/')
            .fold(self.plan.install_dir.clone(), |path, part| path.join(part))
    }
}
