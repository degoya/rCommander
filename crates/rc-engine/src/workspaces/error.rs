//! Why a `workspaces.*` command was refused (RC-100-68), each with a stable code; the window
//! explains it under `error.<code>`.

use super::{SPEC_VERSION, WorkspaceId};
use crate::{ConnectionId, EngineError};

/// A refusal of a workspace or its spec.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WorkspaceError {
    /// The spec is of another version than this build reads.
    #[error(
        "the workspace has version {found}; this rCommander reads version {}",
        SPEC_VERSION
    )]
    Version { found: u64 },
    /// No tab, more than [`super::MAX_TABS`], or an active tab that is not there.
    #[error("the workspace's tabs do not fit: {reason}")]
    Tabs { reason: String },
    /// The terminal view without a terminal, or a terminal that does not fit.
    #[error("the workspace's terminal does not fit: {reason}")]
    Terminal { reason: String },
    /// A tab inside an archive, or a folder that does not fit.
    #[error("the workspace's folder does not fit: {reason}")]
    Location { reason: String },
    /// The spec names a connection the connection tree does not have.
    #[error("there is no connection {}", id.0)]
    Connection { id: ConnectionId },
    /// A theme this system does not offer, or one of the other mode.
    #[error("there is no {mode} theme {id:?}")]
    Theme { id: String, mode: &'static str },
    /// The spec is larger than [`super::MAX_SPEC_BYTES`].
    #[error("the workspace has {bytes} bytes; at most {max} are kept")]
    TooLarge { bytes: usize, max: usize },
    /// The spec does not read: a field missing or of the wrong kind, a divider out of bounds.
    #[error("the workspace does not read: {reason}")]
    Invalid { reason: String },
    /// A name that is empty, too long or holds a control character.
    #[error("the name does not fit: {reason}")]
    Name { reason: String },
    /// Another workspace has this name (case does not count).
    #[error("another workspace is named {name:?}")]
    NameTaken { name: String },
    /// No workspace has this id.
    #[error("there is no workspace {id}")]
    Unknown { id: WorkspaceId },
}

impl WorkspaceError {
    /// Every code of these refusals.
    pub const ALL_CODES: &[&str] = &[
        "workspace_version",
        "workspace_tabs",
        "workspace_terminal",
        "workspace_location",
        "workspace_connection",
        "workspace_theme",
        "workspace_too_large",
        "workspace_invalid",
        "workspace_name",
        "workspace_name_taken",
        "workspace_unknown",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Version { .. } => "workspace_version",
            Self::Tabs { .. } => "workspace_tabs",
            Self::Terminal { .. } => "workspace_terminal",
            Self::Location { .. } => "workspace_location",
            Self::Connection { .. } => "workspace_connection",
            Self::Theme { .. } => "workspace_theme",
            Self::TooLarge { .. } => "workspace_too_large",
            Self::Invalid { .. } => "workspace_invalid",
            Self::Name { .. } => "workspace_name",
            Self::NameTaken { .. } => "workspace_name_taken",
            Self::Unknown { .. } => "workspace_unknown",
        }
    }
}

impl From<WorkspaceError> for EngineError {
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}
