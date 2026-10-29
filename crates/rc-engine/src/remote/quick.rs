//! The quick connection, Total Commander's "New connection" (Ctrl+N): an SSH server reached by
//! host, port, user and a password or key file, without an entry in the connection tree.
//!
//! An unsaved one lives only in memory, under a negative [`ConnectionId`] that the database never
//! hands out: the session pool, `sftp://`, the drive bar and the operations queue take it like any
//! connection. Its password and passphrase are [`SecretString`]s in memory - never in the
//! database, the log or an answer - and go with it when the engine ends. Saved ("Save as
//! connection"), it becomes a connection of the tree's root level and its secrets go into the
//! vault, which must be unlocked (or created) first: nothing is written while it cannot take them.
//! The host key goes through the trust store and its question for a person, as always.
//! `connections.quick_connect` is `GuiOnly`: no MCP call reaches an ad-hoc host with credentials.
//! Nor does MCP see a quick connection by its id ([`Engine::quick_for_call`]): no agent logs in
//! to one, reads its session or closes it. `connections.disconnect` from the window closes one
//! and forgets it, its secrets with it.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use rc_remote::{
    SshSession,
    ssh::{CredentialError, Credentials, KeyMaterial, SessionOptions, SshTarget},
};
use rc_vault::{SecretString, VaultStateKind};
use tokio_util::sync::CancellationToken;

use crate::{
    AuthMethod, ConnectionDraft, ConnectionError, ConnectionId, ConnectionKind, Drive, DriveGroup,
    DriveKind, DriveState, Engine, EngineError, Outcome, QuickConnectRequest, SecretKind,
    SecretParam, sessions::EngineGate,
};

/// The SSH port when none is given.
const SSH_PORT: u16 = 22;

/// An unsaved quick connection.
pub(crate) struct QuickConnection {
    /// `user@host` or `user@host:port`: what the drive bar and the path bar show.
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    password: Option<SecretString>,
    key_path: Option<String>,
    passphrase: Option<SecretString>,
}

/// The quick connections of this run, by their negative ids.
#[derive(Default)]
pub(crate) struct QuickConnections {
    open: Mutex<HashMap<ConnectionId, Arc<QuickConnection>>>,
    /// The last id handed out; the next is one below.
    last: AtomicI64,
}

impl QuickConnections {
    fn open(&self) -> MutexGuard<'_, HashMap<ConnectionId, Arc<QuickConnection>>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The quick connection's secrets, from memory; a key file is read when its turn comes.
struct QuickCredentials {
    connection: Arc<QuickConnection>,
}

#[async_trait]
impl Credentials for QuickCredentials {
    async fn password(&self) -> Result<Option<SecretString>, CredentialError> {
        Ok(self.connection.password.clone())
    }

    async fn private_key(&self) -> Result<Option<KeyMaterial>, CredentialError> {
        let Some(path) = self.connection.key_path.as_deref() else {
            return Ok(None);
        };
        let key = crate::commands::read_local_key_file(path)
            .await
            .map_err(|error| CredentialError::Failed {
                code: error.code(),
                message: error.to_string(),
            })?;
        Ok(Some(KeyMaterial {
            key,
            passphrase: self.connection.passphrase.clone(),
        }))
    }
}

/// The typed text, `None` when nothing was typed.
fn given(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// A secret, `None` when nothing was typed.
fn typed(secret: Option<&SecretParam>) -> Option<&SecretParam> {
    secret.filter(|secret| !secret.is_empty())
}

impl Engine {
    /// `connections.quick_connect`: connects to the server of `request` and shows its start
    /// directory (the home) in the panel. Unsaved, the connection lives in memory; with
    /// `save_as`, it is created in the tree and its secrets are stored in the vault first.
    ///
    /// # Errors
    ///
    /// The host, port or user is missing or invalid; the vault is not unlocked for a save
    /// (`vault_locked`, or `vault_absent` before the first secret); the name is taken; the login
    /// fails with the codes of `connections.connect` (`host_key_unknown` files the question for
    /// a person); the server offers no SFTP.
    pub(crate) async fn quick_connect(
        &self,
        request: QuickConnectRequest,
    ) -> Result<Outcome, EngineError> {
        let panel = self.resolve(request.panel);
        let host = given(Some(request.host.as_str())).ok_or(ConnectionError::HostEmpty)?;
        if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ConnectionError::HostInvalid.into());
        }
        let port = match request.port {
            Some(0) => return Err(ConnectionError::PortInvalid { port: 0 }.into()),
            Some(port) => port,
            None => SSH_PORT,
        };
        let user = given(Some(request.user.as_str())).ok_or(ConnectionError::UsernameEmpty)?;
        // Sent to the server and shown as it is, as a saved connection's user (RC-080-51).
        if user.chars().any(char::is_control) {
            return Err(ConnectionError::ControlCharacter { field: "user name" }.into());
        }
        let key_path = given(request.key_path.as_deref());
        let password = typed(request.password.as_ref());
        let passphrase = typed(request.passphrase.as_ref());
        let id = match given(request.save_as.as_deref()) {
            Some(name) => {
                let draft = ConnectionDraft {
                    kind: ConnectionKind::Ssh,
                    port: Some(i64::from(port)),
                    username: Some(user),
                    auth_method: if key_path.is_some() {
                        AuthMethod::Key
                    } else {
                        AuthMethod::Password
                    },
                    key_path,
                    ..ConnectionDraft::new(name, host)
                };
                self.save_quick(&draft, password, passphrase).await?
            }
            None => self.add_quick(QuickConnection {
                name: if port == SSH_PORT {
                    format!("{user}@{host}")
                } else {
                    format!("{user}@{host}:{port}")
                },
                host,
                port,
                user,
                password: password.map(|secret| secret.secret().clone()),
                key_path,
                passphrase: passphrase.map(|secret| secret.secret().clone()),
            }),
        };
        let start = match self.sftp_start(id, true).await {
            Ok(start) => start,
            Err(error) => {
                // A failed quick connection is not kept: the next try types it again.
                self.forget_quick(id);
                return Err(error);
            }
        };
        self.publish(crate::EngineEvent::Drives);
        self.show_path(panel, start).await
    }

    /// Creates the connection of `draft` and stores its secrets; the vault is asked first, so a
    /// locked one leaves the tree as it was. A secret that cannot be stored takes the new
    /// connection with it: none is left without what it was saved with (RC-080-51).
    async fn save_quick(
        &self,
        draft: &ConnectionDraft,
        password: Option<&SecretParam>,
        passphrase: Option<&SecretParam>,
    ) -> Result<ConnectionId, EngineError> {
        if password.is_some() || passphrase.is_some() {
            match self.vault_handle().await?.status().state {
                VaultStateKind::Unlocked => {}
                VaultStateKind::Locked => return Err(EngineError::VaultLocked),
                VaultStateKind::Absent => return Err(rc_vault::VaultError::Absent.into()),
            }
        }
        let Outcome::Connection { connection } = self.create_connection(None, draft).await? else {
            return Err(EngineError::DatabaseUnavailable);
        };
        for (kind, secret) in [
            (SecretKind::Password, password),
            (SecretKind::Passphrase, passphrase),
        ] {
            if let Some(secret) = secret
                && let Err(error) = self.set_secret(connection.id, kind, secret).await
            {
                if let Err(undo) = self.delete_connection(connection.id).await {
                    tracing::warn!(
                        connection = %connection.id,
                        error = %undo,
                        "a saved quick connection whose secret failed could not be removed"
                    );
                }
                return Err(error);
            }
        }
        Ok(connection.id)
    }

    /// Keeps `connection` under a new negative id.
    fn add_quick(&self, connection: QuickConnection) -> ConnectionId {
        let quick = &self.inner.remotes.quick;
        let id = ConnectionId(quick.last.fetch_sub(1, Ordering::SeqCst) - 1);
        quick.open().insert(id, Arc::new(connection));
        id
    }

    /// The quick connection `id`, if it is one.
    pub(crate) fn quick_connection(&self, id: ConnectionId) -> Option<Arc<QuickConnection>> {
        self.inner.remotes.quick.open().get(&id).cloned()
    }

    /// The quick connection `id` for the command running now, if it is one.
    ///
    /// # Errors
    ///
    /// An MCP call: `connection_not_found`, as for any id the tree does not hold.
    pub(crate) fn quick_for_call(
        &self,
        id: ConnectionId,
    ) -> Result<Option<Arc<QuickConnection>>, EngineError> {
        match self.quick_connection(id) {
            Some(_) if crate::read_guard::from_mcp() => {
                Err(ConnectionError::ConnectionNotFound { id }.into())
            }
            found => Ok(found),
        }
    }

    /// The quick connection the drive bar names `name`, for the command running now: what
    /// `sftp://<name>/...` means when no saved connection has that name; of several with the
    /// name the newest, the one typed last. Never for MCP.
    pub(crate) fn quick_named(&self, name: &str) -> Option<ConnectionId> {
        if crate::read_guard::from_mcp() {
            return None;
        }
        self.inner
            .remotes
            .quick
            .open()
            .iter()
            .filter(|(_, connection)| connection.name == name)
            // Ids count down: the newest has the lowest.
            .map(|(id, _)| *id)
            .min()
    }

    /// Drops the quick connection `id` and its secrets, with its SFTP backend.
    pub(crate) fn forget_quick(&self, id: ConnectionId) {
        if self.inner.remotes.quick.open().remove(&id).is_some() {
            self.forget_sftp(id);
        }
    }

    /// The drive bar's entries of the quick connections, oldest first.
    pub(crate) fn quick_drives(&self) -> Vec<Drive> {
        let mut quick: Vec<(ConnectionId, String)> = self
            .inner
            .remotes
            .quick
            .open()
            .iter()
            .map(|(id, connection)| (*id, connection.name.clone()))
            .collect();
        quick.sort_by_key(|(id, _)| std::cmp::Reverse(*id));
        quick
            .into_iter()
            .map(|(id, name)| Drive {
                id: format!("server:{id}"),
                group: DriveGroup::Servers,
                kind: DriveKind::Server,
                label: name,
                root: Some(rc_vfs::VfsPath {
                    backend: super::sftp::backend(id),
                    path: "/".into(),
                }),
                state: DriveState::Unknown,
                volume: None,
                space: None,
                folder: Vec::new(),
                favourite: true,
                remote: None,
            })
            .collect()
    }

    /// A session to the quick connection `connection`, outside the pool.
    pub(crate) async fn open_quick_session(
        &self,
        connection: Arc<QuickConnection>,
        cancel: &CancellationToken,
    ) -> Result<SshSession, EngineError> {
        let target = SshTarget {
            host: connection.host.clone(),
            port: connection.port,
            user: connection.user.clone(),
        };
        let options = SessionOptions {
            keepalive: Duration::from_secs(u64::from(crate::sessions::DEFAULT_KEEPALIVE_SECS)),
            agents: self.agent_sources().await?,
            max_agent_keys: self.agent_max_keys().await,
            ..SessionOptions::default()
        };
        let gate = Arc::new(EngineGate {
            engine: self.clone(),
        });
        let credentials = QuickCredentials { connection };
        Ok(SshSession::connect(&target, &options, gate, &credentials, cancel).await?)
    }
}

#[cfg(test)]
#[path = "quick_tests.rs"]
mod tests;
