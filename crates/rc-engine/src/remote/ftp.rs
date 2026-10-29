//! An FTP connection of the tree as an [`FtpVfs`]: its fields as an [`FtpConfig`], its password
//! from the vault.
//!
//! The kind is the encryption (`ftp` none, `ftps` explicit `AUTH TLS`, `ftps_implicit` TLS from
//! the first byte). An FTPS session trusts what the system roots vouch for and the certificate a
//! person trusted for the server (`certs.rs`, RC-070-02); any other is refused before the login
//! (`certificate_unknown`, `certificate_changed` or `certificate_expired`, with its SHA-256
//! fingerprint) and becomes a question for the window. The password goes from the vault into
//! the configuration and leaves only in `PASS`, after the handshake; anonymous logins touch the
//! vault not at all.
//!
//! In a panel (RC-070-03) the session is kept and registered with the operations queue, as an
//! SFTP backend is, so F5 and F6 reach the server; it opens in the connection's start directory,
//! else where the login started (`PWD`).

use std::{path::Path, sync::Arc};

use rc_ops::Backend;
use rc_remote::ftp::{
    Credentials, DataMode, Encryption, FtpConfig, FtpError, FtpVfs, NameEncoding,
};
use rc_vault::SecretString;
use rc_vfs::{BackendId, Vfs, VfsPath};

use crate::{
    AuthMethod, Connection, ConnectionError, ConnectionId, ConnectionKind, Engine, EngineError,
    FtpDataMode, FtpEncoding, SecretKind, SecretRef,
    remote::fold,
    settings::{FTP_RESOLVE_LINKS_KEY, stored_bool},
};

/// How often [`Engine::open_ftp`] logs in again when kept sessions were dropped meanwhile.
const OPEN_TRIES: usize = 3;

/// The backend of the FTP connection `id`.
pub(crate) fn backend(id: ConnectionId) -> BackendId {
    BackendId::Ftp {
        connection_id: id.0,
    }
}

/// How a connection of `kind` is encrypted; `None` for a kind that is not FTP.
pub(crate) fn encryption(kind: ConnectionKind) -> Option<Encryption> {
    match kind {
        ConnectionKind::Ftp => Some(Encryption::None),
        ConnectionKind::Ftps => Some(Encryption::Explicit),
        ConnectionKind::FtpsImplicit => Some(Encryption::Implicit),
        ConnectionKind::Ssh => None,
    }
}

/// What it takes to reach `connection`'s server; `password` for a login with one.
///
/// # Errors
///
/// Not an FTP connection (`ftp_connection_not_found`), a password login without a password
/// (`password_not_stored`) or without a user (`username_empty`).
pub(crate) fn ftp_config(
    connection: &Connection,
    password: Option<SecretString>,
) -> Result<FtpConfig, EngineError> {
    let encryption = encryption(connection.kind).ok_or_else(|| not_ftp(connection))?;
    let credentials = match (connection.auth_method, password) {
        (AuthMethod::Anonymous, _) => Credentials::Anonymous,
        (_, Some(password)) => Credentials::User {
            name: connection
                .username
                .clone()
                .ok_or(ConnectionError::UsernameEmpty)?,
            password,
        },
        (_, None) => {
            return Err(EngineError::PasswordNotStored {
                connection: connection.id,
            });
        }
    };
    let mut config = FtpConfig::new(connection.host.clone(), encryption, credentials);
    config.port = connection.port;
    config.data_mode = match connection.ftp_data_mode.unwrap_or_default() {
        FtpDataMode::Passive => DataMode::Passive,
        FtpDataMode::Active => DataMode::Active,
    };
    config.encoding = match connection.ftp_encoding.unwrap_or_default() {
        FtpEncoding::Latin1 => NameEncoding::Latin1,
        FtpEncoding::Utf8 => NameEncoding::Utf8,
    };
    Ok(config)
}

/// The refusal of a connection that is not an FTP one where an FTP connection is needed.
pub(crate) fn not_ftp(connection: &Connection) -> EngineError {
    EngineError::FtpConnectionNotFound {
        connection: connection.name.clone(),
    }
}

impl Engine {
    /// Connects to the FTP connection `id` and logs in, with the stored password unless it
    /// logs in anonymously. A new session every call; [`Engine::open_vfs`] keeps one. A locked
    /// vault is announced only when `announce` (never for an agent's call over MCP).
    ///
    /// # Errors
    ///
    /// The connection missing or not an FTP one, `vault_locked` (announced as
    /// [`crate::EngineEvent::SecretNeeded`] when `announce`), `password_not_stored`, or the [`FtpError`] code
    /// of the failed connect or login (`ftp_login_failed`, ...); a refused certificate as
    /// `certificate_unknown`, `certificate_changed` or `certificate_expired`, after filing its
    /// question ([`Engine::certificate_refused`]).
    ///
    /// [`FtpError`]: rc_remote::FtpError
    pub(crate) async fn connect_ftp(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<FtpVfs, EngineError> {
        let connection = self.login_connection(id).await?;
        if !connection.kind.is_ftp() {
            return Err(not_ftp(&connection));
        }
        let password = if connection.auth_method == AuthMethod::Anonymous {
            None
        } else {
            self.secret_for(
                SecretRef {
                    connection: id,
                    kind: SecretKind::Password,
                },
                announce,
            )
            .await?
        };
        let mut config = ftp_config(&connection, password)?;
        // Lazily by default: a listing shows links as links (RC-080-46).
        config.resolve_links = match self.db() {
            Ok(db) => stored_bool(db, FTP_RESOLVE_LINKS_KEY).await,
            Err(_) => false,
        };
        let pinned = if config.encryption == Encryption::None {
            None
        } else {
            let (trust, pinned) = self.certificate_trust(&config.host, config.port).await?;
            config.trust = Arc::new(trust);
            pinned
        };
        tracing::debug!(
            connection = %id,
            host = %config.host,
            port = config.port,
            "connecting to an FTP server"
        );
        match FtpVfs::connect(config).await {
            Ok(vfs) => {
                if let Some(pinned) = pinned
                    && let Err(error) = self.db()?.touch_trusted_certificate(pinned).await
                {
                    tracing::warn!(%error, "could not note the use of a trusted certificate");
                }
                Ok(vfs)
            }
            Err(FtpError::CertificateUnknown(rejection)) => {
                Err(self.certificate_refused(*rejection).await)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The kept session of the FTP connection `id`, else a new one ([`Engine::connect_ftp`]),
    /// kept and registered with the operations queue.
    ///
    /// # Errors
    ///
    /// As [`Engine::connect_ftp`]; `cancelled` when kept sessions were dropped during every
    /// one of [`OPEN_TRIES`] logins.
    pub(crate) async fn open_ftp(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<FtpVfs, EngineError> {
        let remotes = &self.inner.remotes;
        for _ in 0..OPEN_TRIES {
            let forgets = {
                let kept = remotes.ftp();
                if let Some(open) = kept.get(&id.0) {
                    return Ok(open.clone());
                }
                remotes.ftp_forgets()
            };
            let opened = self.connect_ftp(id, announce).await?;
            let kept = {
                let mut kept = remotes.ftp();
                if remotes.ftp_forgets() != forgets {
                    None
                } else if let Some(other) = kept.get(&id.0) {
                    // Another call opened one meanwhile: the first one stays.
                    Some(Err(other.clone()))
                } else {
                    // Registered under the lock too, so a drop now takes all of it.
                    kept.insert(id.0, opened.clone());
                    self.register_ftp(id, &opened);
                    Some(Ok(()))
                }
            };
            match kept {
                Some(Ok(())) => {
                    self.warn_if_plain_ftp(id).await;
                    return Ok(opened);
                }
                Some(Err(other)) => {
                    opened.disconnect().await;
                    return Ok(other);
                }
                // Dropped meanwhile (the connection changed, a disconnect): its login may hold
                // old data, so it closes and the next try logs in with what is stored now.
                None => opened.disconnect().await,
            }
        }
        Err(EngineError::Cancelled)
    }

    /// Registers the kept session `kept` of `id` as its backend and with the queue.
    fn register_ftp(&self, id: ConnectionId, kept: &FtpVfs) {
        let as_vfs: Arc<dyn Vfs> = Arc::new(kept.clone());
        self.inner
            .remotes
            .open()
            .insert(backend(id), Arc::clone(&as_vfs));
        self.inner.ops.register(backend(id), Backend::new(as_vfs));
    }

    /// Where a panel starts on the FTP connection `id`: its start directory, else the directory
    /// the server's login starts in.
    ///
    /// # Errors
    ///
    /// As [`Engine::open_ftp`].
    pub(crate) async fn ftp_start(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<VfsPath, EngineError> {
        let start = self.db()?.connection(id).await?.start_directory;
        let path = match start.as_deref() {
            Some(start) if start.starts_with('/') => fold(start),
            _ => fold_home(self.open_ftp(id, announce).await?.home()),
        };
        Ok(VfsPath {
            backend: backend(id),
            path,
        })
    }

    /// Drops the kept session of the FTP connection `id` and takes it from the operations
    /// queue, then closes it ([`FtpVfs::disconnect`]): its idle connections say `QUIT` at once,
    /// whoever still holds a clone; running transfers finish their call. Whether one was kept.
    pub(crate) async fn forget_ftp(&self, id: ConnectionId) -> bool {
        let kept = {
            let mut kept = self.inner.remotes.ftp();
            self.inner.remotes.note_ftp_forget();
            kept.remove(&id.0)
        };
        self.inner.remotes.open().remove(&backend(id));
        self.inner.ops.unregister(&backend(id));
        match kept {
            Some(session) => {
                session.disconnect().await;
                true
            }
            None => false,
        }
    }

    /// Drops and closes every kept FTP session.
    pub(crate) async fn forget_all_ftp(&self) {
        let kept: Vec<(i64, FtpVfs)> = {
            let mut kept = self.inner.remotes.ftp();
            self.inner.remotes.note_ftp_forget();
            kept.drain().collect()
        };
        for (id, session) in kept {
            self.inner.ops.unregister(&backend(ConnectionId(id)));
            session.disconnect().await;
        }
    }
}

/// The server's home as a panel path: `..` folded, as every remote path is.
fn fold_home(home: &Path) -> std::path::PathBuf {
    fold(&home.to_string_lossy())
}

#[cfg(test)]
#[path = "ftp_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ftp_panel_tests.rs"]
mod panel_tests;

#[cfg(all(test, unix))]
#[path = "ftp_links_tests.rs"]
mod links_tests;

#[cfg(test)]
#[path = "ftp_live_tests.rs"]
mod live_tests;

#[cfg(test)]
#[path = "ftp_live_ops_tests.rs"]
mod live_ops_tests;

#[cfg(test)]
#[path = "ftp_live_s2s_tests.rs"]
mod live_s2s_tests;

#[cfg(test)]
#[path = "ftp_race_tests.rs"]
mod race_tests;
