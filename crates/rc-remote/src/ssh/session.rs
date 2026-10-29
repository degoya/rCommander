//! One SSH connection, set up and authenticated, that channels are opened on.
//!
//! [`SshSession::connect`] connects over TCP, runs the handshake with the host-key gate, and
//! logs in - each step under its own timeout, the whole of it cancellable. The session then
//! carries any number of channels at once (SFTP, a terminal, a command) over its one TCP
//! connection. russh sends a keepalive after [`SessionOptions::keepalive`] of silence and gives
//! the connection up after [`KEEPALIVE_MAX`] unanswered ones; [`SshSession::closed`] resolves
//! then, and a channel opened afterwards fails with `connection_lost`.

use std::{borrow::Cow, sync::Arc, time::Duration};

use russh::{ChannelMsg, Disconnect, client};
use tokio::{net::TcpStream, sync::watch, time::timeout};
use tokio_util::sync::CancellationToken;

use crate::{
    OfferedKey,
    hostkey::preferred_key_algorithms,
    ssh::{
        AuthUsed, Credentials, HostKeyGate, KEEPALIVE_MAX, PtyStream, SessionOptions, SshChannel,
        SshError, SshTarget, Stage, auth,
        handler::{ClientHandler, Decision, take},
    },
};

/// An authenticated SSH connection.
pub struct SshSession {
    handle: client::Handle<ClientHandler>,
    closed: watch::Receiver<bool>,
    auth: AuthUsed,
    host_key: OfferedKey,
}

impl SshSession {
    /// Connects to `target`, checks its host key with `gate` and logs in with the methods of
    /// `options`, asking `credentials` for a secret only when its method's turn comes. Ends
    /// with [`SshError::Cancelled`] as soon as `cancel` fires.
    ///
    /// # Errors
    ///
    /// No user name, the server unreachable, a timeout, a refused host key (the gate's verdict
    /// in [`SshError::HostKey`]), a locked vault, a key that does not open, no accepted method,
    /// a protocol failure, or the cancellation.
    pub async fn connect(
        target: &SshTarget,
        options: &SessionOptions,
        gate: Arc<dyn HostKeyGate>,
        credentials: &dyn Credentials,
        cancel: &CancellationToken,
    ) -> Result<Self, SshError> {
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(SshError::Cancelled),
            session = Self::establish(target, options, gate, credentials) => session,
        }
    }

    async fn establish(
        target: &SshTarget,
        options: &SessionOptions,
        gate: Arc<dyn HostKeyGate>,
        credentials: &dyn Credentials,
    ) -> Result<Self, SshError> {
        let user = target.user.trim();
        if user.is_empty() {
            return Err(SshError::NoUsername);
        }
        let host = target.host.trim();
        let stream = step(
            Stage::Connect,
            options.connect_timeout,
            TcpStream::connect((host, target.port)),
        )
        .await?
        .map_err(|error| SshError::Unreachable {
            host: host.to_owned(),
            port: target.port,
            reason: error.kind().to_string(),
        })?;
        // The stored key types first: a server with a key of such a type presents that one.
        let known = gate.known_types(host, target.port).await;
        let (closed_tx, closed) = watch::channel(false);
        let decision: Decision = Arc::default();
        let handler = ClientHandler {
            host: host.to_owned(),
            port: target.port,
            gate,
            decision: Arc::clone(&decision),
            closed: closed_tx,
        };
        let mut config = client::Config {
            keepalive_interval: Some(options.keepalive),
            keepalive_max: KEEPALIVE_MAX,
            nodelay: true,
            ..client::Config::default()
        };
        config.preferred.key = Cow::Owned(preferred_key_algorithms(&known, &config.preferred.key));
        let config = Arc::new(config);
        let handshake = step(
            Stage::Handshake,
            options.handshake_timeout,
            client::connect_stream(config, stream, handler),
        )
        .await?;
        let mut handle = match handshake {
            Ok(handle) => handle,
            // A refused key comes back as a bare protocol error; the recorded verdict says why.
            Err(error) => {
                return Err(match take(&decision) {
                    Some(Err(refusal)) => refusal,
                    _ => SshError::transport(&error),
                });
            }
        };
        let host_key = match take(&decision) {
            Some(Ok(offered)) => offered,
            Some(Err(refusal)) => return Err(refusal),
            // A session that came up without the check would have bypassed the trust store.
            None => {
                return Err(SshError::Protocol {
                    reason: "the server's host key was not checked".to_owned(),
                });
            }
        };
        let auth = step(
            Stage::Auth,
            options.auth_timeout,
            auth::authenticate(&mut handle, user, options, credentials),
        )
        .await??;
        tracing::info!(host, port = target.port, method = auth.label(), "SSH login");
        Ok(Self {
            handle,
            closed,
            auth,
            host_key,
        })
    }

    /// A new session channel on this connection: for SFTP, a terminal or a command.
    ///
    /// # Errors
    ///
    /// [`SshError::ConnectionLost`] once the connection is gone, a protocol failure when the
    /// server refuses the channel.
    pub async fn open_channel(&self) -> Result<SshChannel, SshError> {
        if self.is_closed() {
            return Err(SshError::ConnectionLost);
        }
        self.handle.channel_open_session().await.map_err(|error| {
            if self.is_closed() {
                SshError::ConnectionLost
            } else {
                SshError::transport(&error)
            }
        })
    }

    /// Whether the connection is gone.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        *self.closed.borrow() || self.handle.is_closed()
    }

    /// Resolves once the connection is gone: closed by either side, or keepalives unanswered.
    pub async fn closed(&self) {
        let mut closed = self.closed.clone();
        // An error means the handler is gone, which is the same.
        let _ = closed.wait_for(|closed| *closed).await;
    }

    /// The connection's end as a signal, `true` once it is gone (what [`Self::closed`] waits
    /// for): a backend on its channels watches it ([`crate::SftpVfs::ending_with`]).
    #[must_use]
    pub fn closed_signal(&self) -> watch::Receiver<bool> {
        self.closed.clone()
    }

    /// Says goodbye to the server; the connection then ends.
    pub async fn disconnect(&self) {
        // A connection that is gone already needs no goodbye.
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
    }

    /// The method the login used.
    #[must_use]
    pub fn auth(&self) -> &AuthUsed {
        &self.auth
    }

    /// The host key the server presented and the gate accepted.
    #[must_use]
    pub fn host_key(&self) -> &OfferedKey {
        &self.host_key
    }
}

/// Requests a pseudo-terminal of `cols` by `rows` and a shell on `channel` and turns it into a
/// byte stream: what the remote terminal (RC-060-03) and the login scripts (RC-050-08) drive.
///
/// # Errors
///
/// The server refuses the terminal or the shell ([`SshError::Protocol`]), or the connection is
/// gone.
pub async fn open_pty_shell(
    mut channel: SshChannel,
    term: &str,
    cols: u32,
    rows: u32,
) -> Result<PtyStream, SshError> {
    start_pty_shell(&mut channel, term, cols, rows).await?;
    Ok(channel.into_stream())
}

/// Requests a pseudo-terminal of `cols` by `rows` and a shell on `channel`, which stays a
/// channel: the terminal panel's back end (RC-060-03) reads its messages itself, for the exit
/// status, and sends `window-change` on it.
///
/// # Errors
///
/// As [`open_pty_shell`].
pub async fn start_pty_shell(
    channel: &mut SshChannel,
    term: &str,
    cols: u32,
    rows: u32,
) -> Result<(), SshError> {
    channel
        .request_pty(true, term, cols, rows, 0, 0, &[])
        .await
        .map_err(|error| SshError::transport(&error))?;
    confirmed(channel, "terminal").await?;
    channel
        .request_shell(true)
        .await
        .map_err(|error| SshError::transport(&error))?;
    confirmed(channel, "shell").await
}

/// Waits for the server's answer to the request just sent on `channel`.
async fn confirmed(channel: &mut SshChannel, what: &str) -> Result<(), SshError> {
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => return Ok(()),
            Some(ChannelMsg::Failure) => {
                return Err(SshError::Protocol {
                    reason: format!("the server refused the {what}"),
                });
            }
            Some(ChannelMsg::Close | ChannelMsg::Eof) | None => {
                return Err(SshError::ConnectionLost);
            }
            Some(_) => {}
        }
    }
}

/// `work` within `limit`, or [`SshError::Timeout`] for `stage`.
async fn step<T>(
    stage: Stage,
    limit: Duration,
    work: impl Future<Output = T>,
) -> Result<T, SshError> {
    timeout(limit, work).await.map_err(|_| SshError::Timeout {
        stage,
        seconds: limit.as_secs(),
    })
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
