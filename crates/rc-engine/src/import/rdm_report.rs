//! What `import.remote_desktop_manager` found, planned and did (RC-050-11). The report repeats
//! names, folders, RDM types, hosts, users, ports and key paths - never a password or its
//! obfuscated form, only [`RdmPassword`].

use rc_db::connections::ImportStatus;
use serde::{Deserialize, Serialize};

use super::rdm::RdmPassword;
use crate::ConnectionKind;

/// The root folder the import fills; RDM's folders go inside.
pub const DEFAULT_FOLDER: &str = "Imported from Remote Desktop Manager";

/// The `import_source` of an imported connection. One for every export file: an entry's key is
/// RDM's own ID, a GUID, so a later export under another name finds the same connections.
pub const SOURCE: &str = "remote_desktop_manager";

/// One entry of the export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdmRow {
    /// RDM's ID in lower case (what `only` names); `entry<N>` for one without.
    pub key: String,
    pub name: String,
    pub folders: Vec<String>,
    /// RDM's `ConnectionType`, such as `SSHShell`, `Ftp` or `RDPConfigured`.
    pub rdm_type: String,
    /// `None` for a type rCommander has no connection for.
    pub kind: Option<ConnectionKind>,
    pub host: String,
    pub port: Option<i64>,
    pub user: Option<String>,
    pub key_path: Option<String>,
    /// The entry names a key file that is no plain local path; it was left out.
    pub key_path_refused: bool,
    /// Whether RDM keeps a password: only ever this, never the value. Not named `password`,
    /// which the MCP mask would blank.
    pub password_state: RdmPassword,
    pub selected: bool,
    /// `refused` carries a connection-tree code or an [`super::rdm::RdmSkip`] code.
    #[serde(flatten)]
    pub status: ImportStatus,
}

/// What `import.remote_desktop_manager` read, planned and did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdmImportReport {
    /// The file read.
    pub path: String,
    pub dry_run: bool,
    pub entries: Vec<RdmRow>,
    /// Codes of what was not read: `too_many_entries` (more than
    /// [`super::rdm::MAX_ENTRIES`]).
    pub warnings: Vec<String>,
}
