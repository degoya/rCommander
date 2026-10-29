//! What the connection tree refuses or fails at, each with a stable code.

use crate::{
    DbError,
    connections::{AuthMethod, ConnectionId, ConnectionKind, CredentialId, FolderId},
};

/// A refused or failed change to the connection tree.
///
/// [`ConnectionError::code`] is the stable form the GUI looks its message up under
/// (`error.<code>`) and the MCP server answers with. A code never changes once released.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConnectionError {
    /// No connection has this id.
    #[error("there is no connection with id {id}")]
    ConnectionNotFound { id: ConnectionId },
    /// No folder has this id.
    #[error("there is no connection folder with id {id}")]
    FolderNotFound { id: FolderId },
    /// No shared credential has this id (RC-050-12).
    #[error("there is no shared credential with id {id}")]
    CredentialNotFound { id: CredentialId },
    /// A name is empty or blank.
    #[error("a name must not be empty")]
    NameEmpty,
    /// Another connection in the folder, or another folder in the parent, has this name
    /// (ignoring ASCII case).
    #[error("the name {name:?} is already taken in this folder")]
    NameTaken { name: String },
    /// The host is empty or blank.
    #[error("the host must not be empty")]
    HostEmpty,
    /// The host contains whitespace or a control character.
    #[error("a host contains no spaces or control characters")]
    HostInvalid,
    /// The port is outside 1 to 65535.
    #[error("port {port} is outside 1 to 65535")]
    PortInvalid { port: i64 },
    /// A folder would move into itself or one of its own subfolders.
    #[error("a folder cannot move into itself or one of its subfolders")]
    FolderCycle,
    /// The folders would nest deeper than [`super::MAX_DEPTH`].
    #[error("folders nest at most {max} levels deep")]
    FolderTooDeep { max: usize },
    /// A folder to delete still holds something, and the delete is not recursive.
    #[error("folder {id} is not empty; delete it recursively or empty it first")]
    FolderNotEmpty { id: FolderId },
    /// The auth method is not one the connection's kind can log in with (FTP: password or
    /// anonymous; SSH: anything but anonymous).
    #[error("a {} connection cannot log in with {}", .kind.as_str(), .auth_method.as_str())]
    AuthUnsupported {
        kind: ConnectionKind,
        auth_method: AuthMethod,
    },
    /// An FTP connection logs in with a password and names no user.
    #[error("an FTP connection that logs in with a password needs a user name")]
    UsernameEmpty,
    /// The default view is one the connection's kind has not: FTP opens no terminal.
    #[error("a {} connection opens only as files", .kind.as_str())]
    ViewUnsupported { kind: ConnectionKind },
    /// A name, user name or start directory holds a control character (a line break, an escape):
    /// it would be typed into a terminal or sent to a server as it is.
    #[error("the {field} contains a control character")]
    ControlCharacter { field: &'static str },
    /// A name, host or user name is longer than [`super::MAX_TEXT_CHARS`] characters.
    #[error("the {field} is longer than {max} characters")]
    TextTooLong { field: &'static str, max: usize },
    /// A key path or start directory is longer than [`super::MAX_PATH_CHARS`] characters.
    #[error("the {field} is longer than {max} characters")]
    PathTooLong { field: &'static str, max: usize },
    /// The change would point a connection that holds a secret at another host, port, kind or
    /// user ([`super::EndpointRule::KeepWhileSecretsHeld`]); only the person in the window
    /// does that. The same code as the engine's own refusal.
    #[error(
        "connection {id} holds a secret; only the person in rCommander's window changes its \
         host, port, kind or user"
    )]
    EndpointChangeNeedsGui { id: ConnectionId },
    /// The database failed.
    #[error(transparent)]
    Db(#[from] DbError),
}

impl ConnectionError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        "connection_not_found",
        "folder_not_found",
        "credential_not_found",
        "name_empty",
        "name_taken",
        "host_empty",
        "host_invalid",
        "port_invalid",
        "folder_cycle",
        "folder_too_deep",
        "folder_not_empty",
        "auth_unsupported",
        "username_empty",
        "view_unsupported",
        "control_character",
        "text_too_long",
        "path_too_long",
        "endpoint_change_needs_gui",
        "database_error",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::ConnectionNotFound { .. } => "connection_not_found",
            Self::FolderNotFound { .. } => "folder_not_found",
            Self::CredentialNotFound { .. } => "credential_not_found",
            Self::NameEmpty => "name_empty",
            Self::NameTaken { .. } => "name_taken",
            Self::HostEmpty => "host_empty",
            Self::HostInvalid => "host_invalid",
            Self::PortInvalid { .. } => "port_invalid",
            Self::FolderCycle => "folder_cycle",
            Self::FolderTooDeep { .. } => "folder_too_deep",
            Self::FolderNotEmpty { .. } => "folder_not_empty",
            Self::AuthUnsupported { .. } => "auth_unsupported",
            Self::UsernameEmpty => "username_empty",
            Self::ViewUnsupported { .. } => "view_unsupported",
            Self::ControlCharacter { .. } => "control_character",
            Self::TextTooLong { .. } => "text_too_long",
            Self::PathTooLong { .. } => "path_too_long",
            Self::EndpointChangeNeedsGui { .. } => "endpoint_change_needs_gui",
            Self::Db(_) => "database_error",
        }
    }
}

impl From<sqlx::Error> for ConnectionError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(DbError::Sqlx(error))
    }
}
