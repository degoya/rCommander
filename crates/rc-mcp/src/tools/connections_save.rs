//! `connections_save`: a connection's fields, login script and drive-bar mark saved in one step
//! over `connections.save` (RC-080-25), all of it or nothing.
//!
//! It takes no secrets, like the other connection tools: a password, key or passphrase among the
//! arguments is refused by the gate (`secret_not_accepted`, `secret_args.rs`), and the engine
//! refuses a secret change over MCP besides (`gui_only`). Making a connection run a login script
//! runs it on a server, so a call that names `login_script_id` is an exec call
//! (`Command::effective_class`, RC-080-42), as `login_scripts_assign` is.

use rc_engine::{
    Command, Connection, ConnectionDraft, ConnectionId, ConnectionKind, ConnectionPatch, FolderId,
    LoginScriptId, Outcome, SaveConnectionRequest, SaveFields,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Deserializer, Serialize};

use super::{
    connections::{AuthArg, KindArg, ViewArg},
    remote::{FtpEncodingArg, FtpModeArg},
};
use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result},
};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SaveParams {
    /// The connection to change, from connections_list; omitted, a new one is added.
    #[serde(default)]
    pub id: Option<i64>,
    /// A new connection's folder; the root when omitted. A stored one moves with
    /// connections_move.
    #[serde(default)]
    pub folder_id: Option<i64>,
    /// Needed for a new connection; unique in its folder, ignoring case.
    #[serde(default)]
    pub name: Option<String>,
    /// Host name or IP address; needed for a new connection.
    #[serde(default)]
    pub host: Option<String>,
    /// 1 to 65535; a new connection's default is 22 for ssh, 21 for ftp and ftps, 990 for
    /// ftps_implicit.
    #[serde(default)]
    pub port: Option<i64>,
    /// ssh (a new connection's default), ftp, ftps or ftps_implicit.
    #[serde(default)]
    pub kind: Option<KindArg>,
    /// The login name; an empty string clears it.
    #[serde(default)]
    pub username: Option<String>,
    /// password (a new connection's default), key, agent, interactive or, for FTP, anonymous.
    /// Only the method: the secret itself is entered by the person in rCommander.
    #[serde(default)]
    pub auth_method: Option<AuthArg>,
    /// Path of a private key file on this machine, never its content; an empty string clears
    /// it.
    #[serde(default)]
    pub key_path: Option<String>,
    /// files (a new connection's default) or terminal.
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
    /// The login script it runs after logging in, from login_scripts_list; null runs none,
    /// omitted keeps it. Naming it makes the call exec: it asks first.
    #[serde(default, deserialize_with = "present")]
    pub login_script_id: Option<Option<i64>>,
    /// true makes it a button of the drive bar among the servers, false leaves it to its
    /// folder's menu; omitted keeps the mark.
    #[serde(default)]
    pub favourite: Option<bool>,
}

/// The answer: the connection as saved.
#[derive(Serialize)]
#[serde(untagged)]
enum Saved {
    Connection(Connection),
    Other(Outcome),
}

/// A field that is there, `null` included: missing stays `None`, `null` is `Some(None)`.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

impl SaveParams {
    /// The engine's request; `Err` for a folder named with a stored connection.
    pub(crate) fn request(self) -> Result<SaveConnectionRequest, &'static str> {
        let fields = match self.id {
            Some(_) if self.folder_id.is_some() => {
                return Err(
                    "folder_id is for a new connection; a stored one moves with connections_move",
                );
            }
            Some(id) => SaveFields::Update {
                id: ConnectionId(id),
                patch: ConnectionPatch {
                    name: self.name,
                    kind: self.kind.map(Into::into),
                    host: self.host,
                    port: self.port,
                    username: self.username,
                    auth_method: self.auth_method.map(Into::into),
                    key_path: self.key_path,
                    default_view: self.default_view.map(Into::into),
                    start_directory: self.start_directory,
                    notes: self.notes,
                    ftp_data_mode: self.ftp_data_mode.map(Into::into),
                    ftp_encoding: self.ftp_encoding.map(Into::into),
                    theme_light: self.theme_light,
                    theme_dark: self.theme_dark,
                },
            },
            None => {
                let kind = self.kind.map_or_else(ConnectionKind::default, Into::into);
                let mut draft = ConnectionDraft::new(
                    self.name.unwrap_or_default(),
                    self.host.unwrap_or_default(),
                );
                draft.kind = kind;
                draft.port = self.port;
                draft.username = self.username;
                if let Some(method) = self.auth_method {
                    draft.auth_method = method.into();
                }
                draft.key_path = self.key_path;
                if let Some(view) = self.default_view {
                    draft.default_view = view.into();
                }
                draft.start_directory = self.start_directory;
                draft.notes = self.notes;
                draft.ftp_data_mode = self.ftp_data_mode.map(Into::into);
                draft.ftp_encoding = self.ftp_encoding.map(Into::into);
                draft.theme_light = self.theme_light;
                draft.theme_dark = self.theme_dark;
                SaveFields::Create {
                    folder_id: self.folder_id.map(FolderId),
                    connection: draft,
                }
            }
        };
        Ok(SaveConnectionRequest {
            fields,
            secrets: Vec::new(),
            login_script: self.login_script_id.map(|id| id.map(LoginScriptId)),
            favourite: self.favourite,
            credential: None,
        })
    }
}

#[tool_router(router = connections_save_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Add or change a connection in one step: its fields, the login script it runs and whether the drive bar lists it, all saved together or not at all. With id it changes that connection (omitted fields stay; an empty string clears username, key_path, start_directory, notes, theme_light or theme_dark); without id it adds one (name and host needed; folder_id or the root). login_script_id: a script from login_scripts_list, null for none, omitted keeps it; naming it is exec and asks first. favourite: true or false for the drive bar, omitted keeps it. Takes no secrets (secret_not_accepted): the person enters them in rCommander. The host, port, kind and username of a connection with a stored secret are changed only in the window (endpoint_change_needs_gui); a script that sends a stored secret is assigned only there (gui_only). Answers the saved connection. Errors: invalid_params, connection_not_found, folder_not_found, name_empty, name_taken, host_empty, host_invalid, port_invalid, auth_unsupported, username_empty, view_unsupported, control_character, text_too_long (a name, host or user over 255 characters), path_too_long (a key path or start directory over 4096 characters), login_script_not_found, theme_unknown, theme_wrong_mode (theme_light, theme_dark: a theme id of that mode from theme_list), endpoint_change_needs_gui, gui_only."
    )]
    pub(crate) async fn connections_save(
        &self,
        Parameters(params): Parameters<SaveParams>,
    ) -> ToolResult {
        let request = match params.request() {
            Ok(request) => request,
            Err(message) => return Ok(error_result("invalid_params", message)),
        };
        self.dispatch_as(Command::SaveConnection(request), |outcome| match outcome {
            Outcome::Connection { connection } => Saved::Connection(connection),
            other => Saved::Other(other),
        })
        .await
    }
}

#[cfg(test)]
#[path = "connections_save_tests.rs"]
mod tests;
