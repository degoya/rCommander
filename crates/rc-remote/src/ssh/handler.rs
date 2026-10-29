//! The russh client callbacks: the host-key check during the key exchange, and the end of the
//! connection.
//!
//! The handler does not decide about a key itself. It reads what the server presented
//! ([`describe`]), asks the [`HostKeyGate`] and records the outcome, because a `false` here
//! comes back from russh as a bare protocol error that cannot say why: the session reads the
//! recorded refusal instead. When the connection ends - a disconnect, unanswered keepalives, or
//! the task simply finishing - the handler is dropped and flips the `closed` signal.
//!
//! No forwarding: the session never asks for agent, X11 or port forwarding (nothing calls
//! russh's `agent_forward`, `request_x11` or `tcpip_forward`), and every channel the server
//! tries to open towards the client - agent, X11, forwarded or direct TCP/IP, Unix sockets, a
//! session - is refused here. russh would accept them by default.

use std::sync::{Arc, Mutex, PoisonError};

use russh::{
    Channel, ChannelOpenFailure,
    client::{self, ChannelOpenHandle, DisconnectReason, Msg, Session},
    keys::PublicKeyOrCertificate,
};
use tokio::sync::watch;

use crate::{
    OfferedKey, describe,
    ssh::{HostKeyGate, SshError},
};

/// What the host-key check decided: the accepted key, or why the connection was refused.
pub(super) type Decision = Arc<Mutex<Option<Result<OfferedKey, SshError>>>>;

/// The callbacks of one session.
pub(super) struct ClientHandler {
    pub host: String,
    pub port: u16,
    pub gate: Arc<dyn HostKeyGate>,
    pub decision: Decision,
    pub closed: watch::Sender<bool>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let decision = match describe(server_public_key) {
            Err(error) => Err(SshError::HostKeyUnreadable(error)),
            Ok(offered) => match self
                .gate
                .check(&self.host, self.port, offered.clone())
                .await
            {
                Ok(()) => Ok(offered),
                Err(refusal) => Err(SshError::HostKey {
                    host: self.host.clone(),
                    port: self.port,
                    offered: Box::new(offered),
                    refusal,
                }),
            },
        };
        let accepted = decision.is_ok();
        *self.decision.lock().unwrap_or_else(PoisonError::into_inner) = Some(decision);
        Ok(accepted)
    }

    async fn server_channel_open_agent_forward(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.refuse("agent forwarding", reply).await;
        Ok(())
    }

    async fn server_channel_open_x11(
        &mut self,
        _channel: Channel<Msg>,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.refuse("X11 forwarding", reply).await;
        Ok(())
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        _channel: Channel<Msg>,
        _connected_address: &str,
        _connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.refuse("port forwarding", reply).await;
        Ok(())
    }

    async fn server_channel_open_direct_tcpip(
        &mut self,
        _channel: Channel<Msg>,
        _host_to_connect: &str,
        _port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.refuse("a direct TCP/IP channel", reply).await;
        Ok(())
    }

    async fn server_channel_open_forwarded_streamlocal(
        &mut self,
        _channel: Channel<Msg>,
        _socket_path: &str,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.refuse("socket forwarding", reply).await;
        Ok(())
    }

    async fn server_channel_open_direct_streamlocal(
        &mut self,
        _channel: Channel<Msg>,
        _socket_path: &str,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.refuse("a direct socket channel", reply).await;
        Ok(())
    }

    async fn server_channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.refuse("a session channel", reply).await;
        Ok(())
    }

    async fn disconnected(
        &mut self,
        reason: DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        self.closed.send_replace(true);
        match reason {
            DisconnectReason::ReceivedDisconnect(info) => {
                tracing::debug!(
                    host = %self.host,
                    port = self.port,
                    reason = ?info.reason_code,
                    "the SSH server closed the connection"
                );
                Ok(())
            }
            DisconnectReason::Error(error) => {
                tracing::debug!(host = %self.host, port = self.port, %error, "SSH connection lost");
                Err(error)
            }
        }
    }
}

impl ClientHandler {
    /// Refuses a channel the server tried to open towards the client.
    async fn refuse(&self, what: &str, reply: ChannelOpenHandle) {
        tracing::debug!(host = %self.host, port = self.port, what, "refused a channel the server opened");
        reply
            .reject(ChannelOpenFailure::AdministrativelyProhibited)
            .await;
    }
}

impl Drop for ClientHandler {
    fn drop(&mut self) {
        self.closed.send_replace(true);
    }
}

/// Takes what the check decided.
pub(super) fn take(decision: &Decision) -> Option<Result<OfferedKey, SshError>> {
    decision
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}
