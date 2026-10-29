//! The connection tree: connections and their folders (RC-050-02), and the shared credentials in
//! it (RC-050-12).

use serde::{Deserialize, Deserializer, Serialize};

use super::{CommandClass, CommandSpec};
use crate::{
    ConnectionDraft, ConnectionId, ConnectionPatch, CredentialDraft, CredentialId, CredentialPatch,
    FolderId, LoginScriptId, SecretKind, SecretParam,
};

/// The parameters of `connections.move`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveConnectionRequest {
    pub id: ConnectionId,
    /// The folder to move it into; the root when `None`.
    #[serde(default)]
    pub folder_id: Option<FolderId>,
    /// Its place among the folder's connections; after the last one when `None`.
    #[serde(default)]
    pub sort_order: Option<i64>,
}

/// The parameters of `folders.move`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveFolderRequest {
    pub id: FolderId,
    /// The folder to move it into; the root when `None`.
    #[serde(default)]
    pub parent_id: Option<FolderId>,
    /// Its place among the parent's folders; after the last one when `None`.
    #[serde(default)]
    pub sort_order: Option<i64>,
}

/// What `connections.save` does to the connection's own fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveFields {
    /// A new connection.
    Create {
        /// The folder to add it to; the root when `None`.
        #[serde(default)]
        folder_id: Option<FolderId>,
        connection: ConnectionDraft,
    },
    /// The changes to a stored one; an empty patch changes no field.
    Update {
        id: ConnectionId,
        #[serde(default)]
        patch: ConnectionPatch,
    },
}

/// One change of a stored secret in `connections.save`; a secret not named is kept. A value is
/// never echoed back: `Debug` and serde write `[redacted]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum SecretChange {
    /// A password or a passphrase to store; a key comes from its file (`key_import_only`).
    Set {
        kind: SecretKind,
        value: SecretParam,
    },
    /// The private key file at `path` taken into the vault with its passphrase, as
    /// `secrets.import_key` does.
    ImportKey {
        path: String,
        #[serde(default)]
        passphrase: Option<SecretParam>,
    },
    /// The stored secret of `kind` removed.
    Clear { kind: SecretKind },
}

/// The parameters of `connections.save`: everything the manager's form saves, applied all at
/// once or not at all (RC-080-25).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveConnectionRequest {
    pub fields: SaveFields,
    /// Applied in this order, after the fields.
    #[serde(default)]
    pub secrets: Vec<SecretChange>,
    /// The login script, when it changes: `null` runs none; left out, it stays.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub login_script: Option<Option<LoginScriptId>>,
    /// Whether the drive bar lists it among the servers, when that changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favourite: Option<bool>,
    /// The shared credential it takes its user and secrets from, when that changes: `null` uses
    /// its own again; left out, it stays (RC-050-12). Never set over MCP.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub credential: Option<Option<CredentialId>>,
}

impl SaveConnectionRequest {
    /// Whether a change stores a secret, so the vault must be open.
    #[must_use]
    pub fn stores_secrets(&self) -> bool {
        self.secrets
            .iter()
            .any(|change| !matches!(change, SecretChange::Clear { .. }))
    }
}

/// What `credentials.save` does to the shared credential's own fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialFields {
    /// A new shared credential.
    Create {
        /// The folder to add it to; the root when `None`.
        #[serde(default)]
        folder_id: Option<FolderId>,
        credential: CredentialDraft,
    },
    /// The changes to a stored one; an empty patch changes no field.
    Update {
        id: CredentialId,
        #[serde(default)]
        patch: CredentialPatch,
    },
}

/// The parameters of `credentials.save`: a shared credential's name, user and secrets, all at
/// once or not at all; secrets only from the window (RC-050-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveCredentialRequest {
    pub fields: CredentialFields,
    /// Applied in this order, after the fields.
    #[serde(default)]
    pub secrets: Vec<SecretChange>,
}

impl SaveCredentialRequest {
    /// Whether a change stores a secret, so the vault must be open.
    #[must_use]
    pub fn stores_secrets(&self) -> bool {
        self.secrets
            .iter()
            .any(|change| !matches!(change, SecretChange::Clear { .. }))
    }
}

/// A field that is there, `null` included: with `#[serde(default)]` a missing one stays `None`
/// and `null` reads as `Some(None)`.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "connections.create",
        class: CommandClass::Write,
        summary: "Add a connection to a folder of the connection tree (no secrets)",
    },
    CommandSpec {
        id: "connections.delete",
        class: CommandClass::Destructive,
        summary: "Delete a connection from the connection tree",
    },
    CommandSpec {
        id: "connections.duplicate",
        class: CommandClass::GuiOnly,
        summary: "Copy a connection with its secrets into its folder, right under it, with a number added to its name",
    },
    CommandSpec {
        id: "connections.get",
        class: CommandClass::Read,
        summary: "One connection: host, port, user, auth method, key path, view, notes",
    },
    CommandSpec {
        id: "connections.list",
        class: CommandClass::Read,
        summary: "The connection tree: folders and connections in display order",
    },
    CommandSpec {
        id: "connections.move",
        class: CommandClass::Write,
        summary: "Move a connection into another folder or to another place in its folder",
    },
    CommandSpec {
        id: "connections.save",
        class: CommandClass::Write,
        summary: "Save a connection in one step: fields, secrets, login script and drive-bar mark, all or nothing",
    },
    CommandSpec {
        id: "connections.update",
        class: CommandClass::Write,
        summary: "Change fields of a connection (no secrets)",
    },
    CommandSpec {
        id: "credentials.delete",
        class: CommandClass::Destructive,
        summary: "Delete a shared credential; the connections that use it fall back to their own secrets",
    },
    CommandSpec {
        id: "credentials.move",
        class: CommandClass::Write,
        summary: "Move a shared credential into another folder of the connection tree",
    },
    CommandSpec {
        id: "credentials.save",
        class: CommandClass::Write,
        summary: "Save a shared credential in one step: name, user and secrets, all or nothing",
    },
    CommandSpec {
        id: "folders.create",
        class: CommandClass::Write,
        summary: "Add a folder to the connection tree",
    },
    CommandSpec {
        id: "folders.delete",
        class: CommandClass::Destructive,
        summary: "Delete a connection folder: empty, or recursive with everything inside",
    },
    CommandSpec {
        id: "folders.move",
        class: CommandClass::Write,
        summary: "Move a connection folder; never into itself or one of its subfolders",
    },
    CommandSpec {
        id: "folders.rename",
        class: CommandClass::Write,
        summary: "Rename a connection folder",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "connections.list" => ListConnections,
            "connections.get" => GetConnection { id: ConnectionId },
            "connections.create" => CreateConnection {
                /// The folder to add it to; the root when `None`.
                #[serde(default)]
                folder_id: Option<FolderId>,
                connection: ConnectionDraft,
            },
            "connections.update" => UpdateConnection {
                id: ConnectionId,
                patch: ConnectionPatch,
            },
            "connections.move" => MoveConnection(MoveConnectionRequest),
            "connections.save" => SaveConnection(SaveConnectionRequest),
            "connections.delete" => DeleteConnection { id: ConnectionId },
            "connections.duplicate" => DuplicateConnection { id: ConnectionId },
            "folders.create" => CreateFolder {
                /// The folder to add it to; the root when `None`.
                #[serde(default)]
                parent_id: Option<FolderId>,
                name: String,
            },
            "folders.rename" => RenameFolder { id: FolderId, name: String },
            "folders.move" => MoveFolder(MoveFolderRequest),
            "folders.delete" => DeleteFolder {
                id: FolderId,
                /// Also delete what the folder holds; without it only an empty folder goes.
                #[serde(default)]
                recursive: bool,
            },
            "credentials.save" => SaveCredential(SaveCredentialRequest),
            "credentials.move" => MoveCredential {
                id: CredentialId,
                /// The folder to move it into; the root when `None`.
                #[serde(default)]
                folder_id: Option<FolderId>,
            },
            "credentials.delete" => DeleteCredential { id: CredentialId },
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "connections_tests.rs"]
mod tests;
