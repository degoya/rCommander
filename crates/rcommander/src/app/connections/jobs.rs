//! The manager's work for the engine, run off the window's thread (inline in the tests): one
//! command whose answer comes back tagged, a save of the form as one `connections.save`, or keys
//! taken into the vault after an import. Every step is a command of the registry; the manager
//! keeps no data of its own.

use rc_engine::{Command, ConnectionId, Engine, Outcome, SaveConnectionRequest, SecretParam};

use super::ConnMsg;

/// What an answer is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    /// `connections.list`: the tree.
    Tree,
    /// `drives.list` or `connections.set_favourite`: which connections the drive bar lists.
    Drives,
    /// `vault.status`.
    Vault,
    /// `login_scripts.list`: the scripts a connection can run.
    Scripts,
    /// A change of the tree: the cursor goes to the folder or connection it answers.
    Select,
    /// A change whose result the tree's events bring.
    Changed,
    /// `connections.connect` of this connection: the session's events bring the result; a
    /// locked vault's refusal runs it again after the unlock.
    Connect(ConnectionId),
    /// `vault.create` or `vault.unlock` from the master-password prompt.
    VaultOpened,
    /// The import's preview (a dry run), by the number of its read (`preview::Held`).
    Preview(u32),
    /// The import itself.
    Imported,
    /// `connections.test` of this connection: the note under the form says how it went.
    Tested(ConnectionId),
    /// `certificates.list`: the trusted FTPS certificates.
    Certificates,
    /// The preview of the import from Total Commander (a dry run), by the number of its read.
    TcPreview(u32),
    /// The import from Total Commander itself.
    TcImported,
    /// Its passwords taken into the vault.
    TcPasswords,
    /// The preview of the import from Remote Desktop Manager (a dry run), by the number of its
    /// read.
    RdmPreview(u32),
    /// The import from Remote Desktop Manager itself.
    RdmImported,
    /// `secrets.sources`: the secret sources, and the shown connection's references.
    Sources,
    /// `secrets.source_search`: the entries for the panel's search, by the number of the search.
    SourceFound(u32),
    /// `secrets.source_map` or `.source_unmap`: the connection's references now.
    SourceMapped,
    /// `credentials.save` of the form: the shared credential as saved (RC-050-12).
    CredentialSaved,
    /// `settings.get` of `vault.shared_credentials`: whether credentials are offered.
    SharedSetting,
    /// `settings.get` of `connections.focus_list`: where the keyboard starts (RC-050-15).
    FocusSetting,
    /// `connections.duplicate` of this connection: the copy, renamed next; a locked vault's
    /// refusal runs it again after the unlock (RC-050-15).
    Duplicated(ConnectionId),
}

/// A key file of an imported connection to take into the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyToTake {
    pub alias: String,
    pub id: ConnectionId,
    pub path: String,
    /// Typed for an encrypted key; `Debug` never shows it.
    pub passphrase: Option<SecretParam>,
}

/// One piece of work.
pub enum ConnJob {
    Run(Then, Box<Command>),
    Save(Box<SaveConnectionRequest>),
    TakeKeys(Vec<KeyToTake>),
}

impl ConnJob {
    /// Runs the job; its answer for the manager.
    pub async fn run(self, engine: Engine) -> ConnMsg {
        match self {
            Self::Run(then, command) => ConnMsg::Answer(
                then,
                Box::new(
                    engine
                        .dispatch(*command)
                        .await
                        .map_err(|error| error.code()),
                ),
            ),
            Self::Save(request) => ConnMsg::Saved(save(&engine, *request).await),
            Self::TakeKeys(keys) => {
                let mut taken = Vec::with_capacity(keys.len());
                for key in keys {
                    let command = Command::ImportKey(rc_engine::ImportKeyRequest {
                        connection_id: key.id,
                        path: key.path,
                        passphrase: key.passphrase,
                    });
                    let result = engine.dispatch(command).await;
                    taken.push((key.alias, result.map(|_| ()).map_err(|error| error.code())));
                }
                ConnMsg::KeysTaken(taken)
            }
        }
    }
}

/// The whole form in one command: stored all of it, or refused with nothing stored.
async fn save(
    engine: &Engine,
    request: SaveConnectionRequest,
) -> Result<ConnectionId, &'static str> {
    match engine.dispatch(Command::SaveConnection(request)).await {
        Ok(Outcome::Connection { connection }) => Ok(connection.id),
        Ok(_) => Err("database_error"),
        Err(error) => Err(error.code()),
    }
}
