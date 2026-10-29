//! The connection tree as the engine sees it: the types of `rc_db::connections`, re-exported so
//! the GUI and the MCP server name them through the engine, and what a change announces.
//!
//! The tree lives in `rcommander.db`; the `connections.*` and `folders.*` commands
//! (`commands/connections.rs`) read and change it through the repository, which validates every
//! write; the `credentials.*` commands (`commands/credentials.rs`) the shared credentials in it.
//! No secret is part of any of these types; a connection or a credential says only which secrets
//! are stored (`has_password`, `has_key`, `has_passphrase`).

use serde::{Deserialize, Serialize};

pub use rc_db::connections::{
    AuthMethod, Connection, ConnectionDraft, ConnectionError, ConnectionFolder, ConnectionId,
    ConnectionKind, ConnectionPatch, ConnectionTree, Credential, CredentialDraft, CredentialId,
    CredentialPatch, DefaultView, FolderDeletion, FolderId, FolderNode, FtpDataMode, FtpEncoding,
    ImportStatus, MAX_DEPTH,
};

/// One node of the tree: `{"connection": 3}`, `{"folder": 1}` or `{"credential": 2}` - a shared
/// credential (RC-050-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeItem {
    Connection(ConnectionId),
    Folder(FolderId),
    Credential(CredentialId),
}

/// What happened to a node. A deleted folder's event covers everything that was inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeChange {
    Created,
    /// Its fields changed (a folder: its name).
    Updated,
    /// It sits in another folder or at another place among its siblings.
    Moved,
    Deleted,
}
