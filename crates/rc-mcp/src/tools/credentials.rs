//! The shared credentials of the connection tree (RC-050-12): thin wrappers over the
//! `credentials.*` commands. `connections_list` lists them with the tree.
//!
//! No tool here takes or answers a secret: a password, key or passphrase among the arguments is
//! refused by the gate (`secret_not_accepted`, `secret_args.rs`), and the engine refuses over MCP
//! what would hand a stored secret to someone else (`gui_only`) - a secret of an entry, a new
//! user of a stored one, and pointing a connection at one.

use rc_engine::{
    Command, Credential, CredentialDraft, CredentialFields, CredentialId, CredentialPatch,
    FolderId, Outcome, SaveCredentialRequest,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result},
};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SaveParams {
    /// The entry to change, from connections_list; omitted, a new one is added.
    #[serde(default)]
    pub id: Option<i64>,
    /// A new entry's folder; the root when omitted. A stored one moves with credentials_move.
    #[serde(default)]
    pub folder_id: Option<i64>,
    /// Needed for a new entry; unique in its folder, ignoring case.
    #[serde(default)]
    pub name: Option<String>,
    /// A new entry's user, the login of every connection on it that names none. A stored
    /// entry's user is changed only in rCommander's window (gui_only).
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct MoveParams {
    /// The entry's id.
    pub id: i64,
    /// The folder id to move it into; the root when omitted.
    #[serde(default)]
    pub folder_id: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct DeleteParams {
    /// The entry's id.
    pub id: i64,
}

/// The answer: the entry itself, or what a delete removed.
#[derive(Serialize)]
#[serde(untagged)]
enum Shown {
    Credential(Credential),
    Other(Outcome),
}

fn shown(outcome: Outcome) -> Shown {
    match outcome {
        Outcome::Credential { credential } => Shown::Credential(credential),
        other => Shown::Other(other),
    }
}

impl SaveParams {
    /// The engine's request; `Err` for a folder named with a stored entry.
    fn request(self) -> Result<SaveCredentialRequest, &'static str> {
        let fields = match self.id {
            Some(_) if self.folder_id.is_some() => {
                return Err(
                    "folder_id is for a new entry; a stored one moves with credentials_move",
                );
            }
            Some(id) => CredentialFields::Update {
                id: CredentialId(id),
                patch: CredentialPatch {
                    name: self.name,
                    username: self.username,
                },
            },
            None => CredentialFields::Create {
                folder_id: self.folder_id.map(FolderId),
                credential: CredentialDraft {
                    name: self.name.unwrap_or_default(),
                    username: self.username,
                },
            },
        };
        Ok(SaveCredentialRequest {
            fields,
            secrets: Vec::new(),
        })
    }
}

#[tool_router(router = credentials_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Add or rename a shared credential of the connection tree: a user and secrets kept once, which connections take their login from. Without id it adds one (name needed, username optional; folder_id or the root); with id it renames that one. Takes no secrets (secret_not_accepted): the person enters them in rCommander. Changing a stored entry's user, and pointing a connection at an entry, happen only in the window (gui_only): the entry's secrets would go wherever an agent chose. Answers the entry: id, folder_id, name, username, has_password, has_key, has_passphrase, key and connections (how many use it). Errors: invalid_params, credential_not_found, folder_not_found, name_empty, name_taken, control_character, text_too_long (a name or user over 255 characters), gui_only."
    )]
    pub(crate) async fn credentials_save(
        &self,
        Parameters(params): Parameters<SaveParams>,
    ) -> ToolResult {
        match params.request() {
            Ok(request) => {
                self.dispatch_as(Command::SaveCredential(request), shown)
                    .await
            }
            Err(message) => Ok(error_result("invalid_params", message)),
        }
    }

    #[tool(
        description = "Move a shared credential into a folder of the connection tree (the root when folder_id is omitted). Errors: credential_not_found, folder_not_found, name_taken."
    )]
    pub(crate) async fn credentials_move(
        &self,
        Parameters(params): Parameters<MoveParams>,
    ) -> ToolResult {
        let command = Command::MoveCredential {
            id: CredentialId(params.id),
            folder_id: params.folder_id.map(FolderId),
        };
        self.dispatch_as(command, shown).await
    }

    #[tool(
        description = "Delete a shared credential with its stored secrets; the connections that used it fall back to their own user and secrets. Destructive: the first call answers with a question and a confirmation code; ask the person, and only if they agree call again with confirmation set to the code. Errors: credential_not_found."
    )]
    pub(crate) async fn credentials_delete(
        &self,
        Parameters(params): Parameters<DeleteParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::DeleteCredential {
            id: CredentialId(params.id),
        })
        .await
    }
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;
