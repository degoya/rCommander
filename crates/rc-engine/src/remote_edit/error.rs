//! The refusals of the remote edits, each with a stable code.

use crate::EngineError;

use super::{MAX_REMOTE_EDITS, RemoteEditId};

/// A remote edit refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RemoteEditError {
    /// [`MAX_REMOTE_EDITS`] server files are open for editing already.
    #[error(
        "{} server files are open for editing already; finish one (Done) first",
        MAX_REMOTE_EDITS
    )]
    TooMany,
    /// No edit has this id (any more).
    #[error("no server file {id} is being edited")]
    Unknown { id: RemoteEditId },
    /// Enter on a server file that would run as a program, while `remote_edit.open_programs` is
    /// off: it is copied first (F5) and started from there.
    #[error("{name} would run as a program; copy it to a local folder first (F5)")]
    Program { name: String },
    /// The file is no plain file: a folder, or a link that leads nowhere or in a circle.
    #[error("{name} is no file that can be edited")]
    NotAFile { name: String },
}

impl RemoteEditError {
    /// Every code of these refusals.
    pub const ALL_CODES: &[&str] = &[
        "remote_edit_too_many",
        "remote_edit_unknown",
        "remote_edit_program",
        "remote_edit_not_a_file",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooMany => "remote_edit_too_many",
            Self::Unknown { .. } => "remote_edit_unknown",
            Self::Program { .. } => "remote_edit_program",
            Self::NotAFile { .. } => "remote_edit_not_a_file",
        }
    }
}

impl From<RemoteEditError> for EngineError {
    fn from(error: RemoteEditError) -> Self {
        Self::RemoteEdit(error)
    }
}
