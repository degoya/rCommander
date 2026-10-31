//! The `connections` tool group: thin wrappers over the `connections.*` and `folders.*`
//! commands.
//!
//! No tool here takes or answers a secret. A call that names one among its arguments (a
//! `password`, `passphrase`, `private_key`, ... at any depth) is refused by the gate with
//! `secret_not_accepted` before the tool runs (`secret_args.rs`), rather than having the field
//! silently ignored: the person enters secrets in rCommander, never through an agent.

use rc_engine::{
    AuthMethod, Command, Connection, ConnectionDraft, ConnectionFolder, ConnectionId,
    ConnectionKind, ConnectionPatch, ConnectionTree, DefaultView, FolderId, Outcome, SessionStatus,
    TreeItem,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use super::remote::{FtpEncodingArg, FtpModeArg};
use crate::{
    RcMcpServer,
    answer::{ToolResult, respond},
};

pub(crate) use super::secret_args::refuse_secrets;

/// A connection's protocol, as tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KindArg {
    Ssh,
    Ftp,
    Ftps,
    FtpsImplicit,
}

impl From<KindArg> for ConnectionKind {
    fn from(kind: KindArg) -> Self {
        match kind {
            KindArg::Ssh => Self::Ssh,
            KindArg::Ftp => Self::Ftp,
            KindArg::Ftps => Self::Ftps,
            KindArg::FtpsImplicit => Self::FtpsImplicit,
        }
    }
}

/// How a connection authenticates, as tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthArg {
    Password,
    Key,
    Agent,
    Interactive,
    Anonymous,
}

impl From<AuthArg> for AuthMethod {
    fn from(method: AuthArg) -> Self {
        match method {
            AuthArg::Password => Self::Password,
            AuthArg::Key => Self::Key,
            AuthArg::Agent => Self::Agent,
            AuthArg::Interactive => Self::Interactive,
            AuthArg::Anonymous => Self::Anonymous,
        }
    }
}

/// What a panel shows when the connection opens, as tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ViewArg {
    Files,
    Terminal,
}

impl From<ViewArg> for DefaultView {
    fn from(view: ViewArg) -> Self {
        match view {
            ViewArg::Files => Self::Files,
            ViewArg::Terminal => Self::Terminal,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct IdParams {
    /// The connection's id, from connections_list.
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct CreateParams {
    /// The name shown in the tree; unique in its folder, ignoring case.
    pub name: String,
    /// Host name or IP address.
    pub host: String,
    /// 1 to 65535; when omitted 22 for ssh, 21 for ftp and ftps, 990 for ftps_implicit.
    #[serde(default)]
    pub port: Option<i64>,
    /// ssh (the default), ftp (plain), ftps (explicit AUTH TLS) or ftps_implicit.
    #[serde(default)]
    pub kind: Option<KindArg>,
    /// The login name.
    #[serde(default)]
    pub username: Option<String>,
    /// password (the default), key, agent or interactive; FTP: password (needs username) or
    /// anonymous. Only the method: the secret itself is entered by the person in rCommander.
    #[serde(default)]
    pub auth_method: Option<AuthArg>,
    /// Path of a private key file on this machine; never the key's content.
    #[serde(default)]
    pub key_path: Option<String>,
    /// files (the default) or terminal: what a panel shows when the connection opens.
    #[serde(default)]
    pub default_view: Option<ViewArg>,
    /// The remote directory a session starts in.
    #[serde(default)]
    pub start_directory: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// The folder id to add it to; the root when omitted.
    #[serde(default)]
    pub folder_id: Option<i64>,
    /// FTP: passive (the default) or active data connections.
    #[serde(default)]
    pub ftp_data_mode: Option<FtpModeArg>,
    /// FTP: latin1 (the default) or utf8 for names on a server without UTF8 in FEAT.
    #[serde(default)]
    pub ftp_encoding: Option<FtpEncodingArg>,
    /// The theme its panel and SSH terminal draw with in the light mode: a light theme's id from
    /// theme_list; omitted, the window's.
    #[serde(default)]
    pub theme_light: Option<String>,
    /// The same for the dark mode: a dark theme's id.
    #[serde(default)]
    pub theme_dark: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateParams {
    /// The connection's id.
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    /// 1 to 65535.
    #[serde(default)]
    pub port: Option<i64>,
    #[serde(default)]
    pub kind: Option<KindArg>,
    /// An empty string clears it.
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub auth_method: Option<AuthArg>,
    /// Path of a private key file, never its content; an empty string clears it.
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub default_view: Option<ViewArg>,
    /// An empty string clears it.
    #[serde(default)]
    pub start_directory: Option<String>,
    /// An empty string clears them.
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub ftp_data_mode: Option<FtpModeArg>,
    #[serde(default)]
    pub ftp_encoding: Option<FtpEncodingArg>,
    /// A light theme's id from theme_list for its panel and SSH terminal; an empty string draws
    /// with the window's again.
    #[serde(default)]
    pub theme_light: Option<String>,
    /// The same for the dark mode: a dark theme's id; an empty string clears it.
    #[serde(default)]
    pub theme_dark: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct MoveParams {
    /// The connection's id.
    pub id: i64,
    /// The folder id to move it into; the root when omitted.
    #[serde(default)]
    pub folder_id: Option<i64>,
    /// Its place among the folder's connections (smaller first); after the last when omitted.
    #[serde(default)]
    pub sort_order: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct DeleteParams {
    /// The connection's id.
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FolderCreateParams {
    /// The folder's name; unique among its parent's folders, ignoring case.
    pub name: String,
    /// The parent folder's id; the root when omitted.
    #[serde(default)]
    pub parent_id: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FolderRenameParams {
    /// The folder's id.
    pub id: i64,
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FolderMoveParams {
    /// The folder's id.
    pub id: i64,
    /// The new parent folder's id; the root when omitted.
    #[serde(default)]
    pub parent_id: Option<i64>,
    /// Its place among the parent's folders (smaller first); after the last when omitted.
    #[serde(default)]
    pub sort_order: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct FolderDeleteParams {
    /// The folder's id.
    pub id: i64,
    /// Also delete every folder and connection inside; without it only an empty folder goes.
    #[serde(default)]
    pub recursive: bool,
}

/// An outcome as these tools answer it.
#[derive(Serialize)]
#[serde(untagged)]
enum View {
    /// The tree with the SSH sessions that are up or being set up.
    Listed {
        #[serde(flatten)]
        tree: ConnectionTree,
        sessions: Vec<SessionStatus>,
    },
    Tree(ConnectionTree),
    Connection(Connection),
    Folder(ConnectionFolder),
    Deleted {
        deleted: TreeItem,
        folders: u64,
        connections: u64,
    },
    Other(Outcome),
}

fn view(outcome: Outcome) -> View {
    match outcome {
        Outcome::ConnectionTree { tree } => View::Tree(tree),
        Outcome::Connection { connection } => View::Connection(connection),
        Outcome::Folder { folder } => View::Folder(folder),
        Outcome::Deleted {
            item,
            folders,
            connections,
        } => View::Deleted {
            deleted: item,
            folders,
            connections,
        },
        other => View::Other(other),
    }
}

impl RcMcpServer {
    async fn run_connections(&self, command: Command) -> ToolResult {
        self.dispatch_as(command, view).await
    }
}

#[tool_router(router = connections_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "The connection tree: folders (id, name, parent_id, sort_order, with their folders, connections and credentials) and the root's connections and credentials, in display order. A connection has id, name, kind, host, port, username, auth_method, key_path, default_view, start_directory, notes, ftp_data_mode and ftp_encoding (FTP; null is passive and latin1), theme_light and theme_dark (the theme ids its panel and SSH terminal draw with in each mode; null is the window's), has_password, has_key, has_passphrase: whether a secret of that kind is stored, key: a private key stored in the vault as {type, fingerprint, comment, stored: true}, or null, and credential_id: the shared credential it takes its secrets (and, without a username of its own, its user) from, or null. A credential has id, folder_id, name, username, has_password, has_key, has_passphrase, key and connections (how many use it). sessions: the connections with an SSH session up or being set up, each with connection_id, state (connecting, connected), login (the method that got in), host_key_fingerprint and channels. Never a password, key or passphrase itself."
    )]
    pub(crate) async fn connections_list(&self) -> ToolResult {
        let engine = &self.context.engine;
        respond(
            engine
                .dispatch_mcp(Command::ListConnections)
                .await
                .map(|outcome| match outcome {
                    Outcome::ConnectionTree { tree } => View::Listed {
                        tree,
                        sessions: engine.ssh_sessions(),
                    },
                    other => view(other),
                }),
        )
    }

    #[tool(
        description = "One connection by id, with has_password, has_key and has_passphrase saying which secrets are stored and key describing a stored private key by type, fingerprint and comment (never key material). Errors: connection_not_found."
    )]
    pub(crate) async fn connections_get(
        &self,
        Parameters(params): Parameters<IdParams>,
    ) -> ToolResult {
        self.run_connections(Command::GetConnection {
            id: ConnectionId(params.id),
        })
        .await
    }

    #[tool(
        description = "Add an SSH, FTP or FTPS connection to the tree, in a folder or at the root. Takes no secrets: a password, key or passphrase among the arguments is refused (secret_not_accepted); the person enters them in rCommander. Answers the new connection. Errors: name_empty, name_taken, host_empty, host_invalid, port_invalid, folder_not_found, auth_unsupported (FTP logs in with password or anonymous, SSH never anonymous), username_empty (FTP with a password), view_unsupported (FTP has no terminal view), control_character (a line break or other control character in the name, username or start_directory), text_too_long (a name, host or user over 255 characters), path_too_long (a key path or start directory over 4096 characters), theme_unknown, theme_wrong_mode (theme_light names a dark theme or theme_dark a light one)."
    )]
    pub(crate) async fn connections_create(
        &self,
        Parameters(params): Parameters<CreateParams>,
    ) -> ToolResult {
        let kind = params.kind.map_or_else(ConnectionKind::default, Into::into);
        let connection = ConnectionDraft {
            name: params.name,
            kind,
            host: params.host,
            port: params.port,
            username: params.username,
            auth_method: params
                .auth_method
                .map_or_else(AuthMethod::default, Into::into),
            key_path: params.key_path,
            default_view: params
                .default_view
                .map_or_else(DefaultView::default, Into::into),
            start_directory: params.start_directory,
            notes: params.notes,
            ftp_data_mode: params.ftp_data_mode.map(Into::into),
            ftp_encoding: params.ftp_encoding.map(Into::into),
            theme_light: params.theme_light,
            theme_dark: params.theme_dark,
        };
        self.run_connections(Command::CreateConnection {
            folder_id: params.folder_id.map(FolderId),
            connection,
        })
        .await
    }

    #[tool(
        description = "Change fields of a connection; omitted fields stay, an empty string clears username, key_path, start_directory, notes, theme_light or theme_dark. A change of the themes alone keeps an open session. Takes no secrets (secret_not_accepted). The host, port, kind and username of a connection with a stored password, key or passphrase are changed only in rCommander's window (endpoint_change_needs_gui): the secret would go to the new endpoint. Answers the changed connection. Errors: connection_not_found, name_empty, name_taken, host_empty, host_invalid, port_invalid, auth_unsupported, username_empty, view_unsupported, control_character, text_too_long, path_too_long, theme_unknown, theme_wrong_mode, endpoint_change_needs_gui."
    )]
    pub(crate) async fn connections_update(
        &self,
        Parameters(params): Parameters<UpdateParams>,
    ) -> ToolResult {
        let patch = ConnectionPatch {
            name: params.name,
            kind: params.kind.map(Into::into),
            host: params.host,
            port: params.port,
            username: params.username,
            auth_method: params.auth_method.map(Into::into),
            key_path: params.key_path,
            default_view: params.default_view.map(Into::into),
            start_directory: params.start_directory,
            notes: params.notes,
            ftp_data_mode: params.ftp_data_mode.map(Into::into),
            ftp_encoding: params.ftp_encoding.map(Into::into),
            theme_light: params.theme_light,
            theme_dark: params.theme_dark,
        };
        self.run_connections(Command::UpdateConnection {
            id: ConnectionId(params.id),
            patch,
        })
        .await
    }

    #[tool(
        description = "Move a connection into a folder (the root when folder_id is omitted), after its last connection or at sort_order. Errors: connection_not_found, folder_not_found, name_taken."
    )]
    pub(crate) async fn connections_move(
        &self,
        Parameters(params): Parameters<MoveParams>,
    ) -> ToolResult {
        self.run_connections(Command::MoveConnection(rc_engine::MoveConnectionRequest {
            id: ConnectionId(params.id),
            folder_id: params.folder_id.map(FolderId),
            sort_order: params.sort_order,
        }))
        .await
    }

    #[tool(
        description = "Delete a connection. Destructive: the first call answers with a question and a confirmation code; ask the person, and only if they agree call again with confirmation set to the code. Errors: connection_not_found."
    )]
    pub(crate) async fn connections_delete(
        &self,
        Parameters(params): Parameters<DeleteParams>,
    ) -> ToolResult {
        self.run_connections(Command::DeleteConnection {
            id: ConnectionId(params.id),
        })
        .await
    }

    #[tool(
        description = "Add a folder to the connection tree, in a parent folder or at the root. Folders nest at most 32 levels. Answers the new folder. Errors: name_empty, name_taken, folder_not_found, folder_too_deep, text_too_long (a name over 255 characters)."
    )]
    pub(crate) async fn connections_folder_create(
        &self,
        Parameters(params): Parameters<FolderCreateParams>,
    ) -> ToolResult {
        self.run_connections(Command::CreateFolder {
            parent_id: params.parent_id.map(FolderId),
            name: params.name,
        })
        .await
    }

    #[tool(
        description = "Rename a connection folder. Errors: folder_not_found, name_empty, name_taken, text_too_long (a name over 255 characters)."
    )]
    pub(crate) async fn connections_folder_rename(
        &self,
        Parameters(params): Parameters<FolderRenameParams>,
    ) -> ToolResult {
        self.run_connections(Command::RenameFolder {
            id: FolderId(params.id),
            name: params.name,
        })
        .await
    }

    #[tool(
        description = "Move a connection folder with everything inside into another folder (the root when parent_id is omitted). Never into itself or one of its subfolders (folder_cycle). Errors: folder_not_found, folder_cycle, folder_too_deep, name_taken, text_too_long."
    )]
    pub(crate) async fn connections_folder_move(
        &self,
        Parameters(params): Parameters<FolderMoveParams>,
    ) -> ToolResult {
        self.run_connections(Command::MoveFolder(rc_engine::MoveFolderRequest {
            id: FolderId(params.id),
            parent_id: params.parent_id.map(FolderId),
            sort_order: params.sort_order,
        }))
        .await
    }

    #[tool(
        description = "Delete a connection folder: an empty one, or with recursive=true everything inside it. Destructive: needs a confirmation code like connections_delete. Answers how many folders and connections went. Errors: folder_not_found, folder_not_empty."
    )]
    pub(crate) async fn connections_folder_delete(
        &self,
        Parameters(params): Parameters<FolderDeleteParams>,
    ) -> ToolResult {
        self.run_connections(Command::DeleteFolder {
            id: FolderId(params.id),
            recursive: params.recursive,
        })
        .await
    }
}

#[cfg(test)]
#[path = "connections_tests.rs"]
mod tests;
