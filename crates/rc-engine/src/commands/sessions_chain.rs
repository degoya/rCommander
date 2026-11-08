//! A session through its jump hosts (RC-110-03, OpenSSH's ProxyJump).
//!
//! The chain is read again from the database at every connect ([`rc_db::Db::jump_chain`]: no
//! cycle, at most [`crate::MAX_JUMPS`], SSH only) and set up outermost first: each hop's session
//! carries the next one's over a `direct-tcpip` channel. Every hop is a connection of its own:
//! its host key is checked under the name and port its connection gives (an unknown one is the
//! person's question, "hop 1 of 3: bastion"; a changed one stops the whole chain before any
//! secret goes to it or a later hop), it logs in with its own methods and secrets, and keeps its
//! own keepalive. The jump sessions belong to the target's session and are not in the pool: a
//! disconnect of the jump host in the window cuts no target off. A failure names its hop
//! ([`EngineError::Hop`]); a locked vault and a cancel stay what they are.

use std::{sync::Arc, time::Duration};

use rc_remote::{
    SshSession,
    ssh::{SessionOptions, SshTarget},
};
use tokio_util::sync::CancellationToken;

use crate::{
    Connection, ConnectionKind, Engine, EngineError, HopLabel, MAX_JUMPS,
    sessions::{EngineCredentials, EngineGate, SessionPrefs},
};

impl Engine {
    /// A new session for the SSH connection `connection`, through its jump hosts when it names
    /// one, outside the pool; and the jump hosts' names, the first one first.
    pub(super) async fn open_chain(
        &self,
        connection: Connection,
        announce: bool,
        cancel: &CancellationToken,
    ) -> Result<(SshSession, Vec<String>), EngineError> {
        let chain = self.db()?.jump_chain(connection.id).await?;
        if chain.len() > MAX_JUMPS {
            return Err(
                rc_db::connections::ConnectionError::JumpChainTooLong { max: MAX_JUMPS }.into(),
            );
        }
        let total = chain.len() + 1;
        let hops: Vec<String> = chain.iter().map(|hop| hop.name.clone()).collect();
        let mut via: Option<Arc<SshSession>> = None;
        for (index, hop) in chain.into_iter().enumerate() {
            let label = HopLabel {
                index: index + 1,
                total,
                name: hop.name.clone(),
            };
            if hop.kind != ConnectionKind::Ssh {
                return Err(rc_db::connections::ConnectionError::JumpNeedsSsh.into());
            }
            let hop = self.with_shared_user(hop).await?;
            let session = self
                .hop_session(&hop, via.take(), Some(label.clone()), announce, cancel)
                .await
                .map_err(|error| error.at_hop(&label))?;
            via = Some(Arc::new(session));
        }
        let label = (total > 1).then(|| HopLabel {
            index: total,
            total,
            name: connection.name.clone(),
        });
        let session = self
            .hop_session(&connection, via, label, announce, cancel)
            .await?;
        Ok((session, hops))
    }

    /// The jump host `connection` with the user of its shared credential when it names none, as
    /// a login takes it ([`Engine::login_connection`] without marking it used).
    async fn with_shared_user(
        &self,
        mut connection: Connection,
    ) -> Result<Connection, EngineError> {
        if connection.username.is_none()
            && let Some(credential) = self.shared_credential(connection.id).await?
        {
            connection.username = credential.username;
        }
        Ok(connection)
    }

    /// One session of a chain: `connection`'s, directly or through `via`, with its own settings,
    /// its own secrets and the trust store's verdict labelled with `hop`.
    async fn hop_session(
        &self,
        connection: &Connection,
        via: Option<Arc<SshSession>>,
        hop: Option<HopLabel>,
        announce: bool,
        cancel: &CancellationToken,
    ) -> Result<SshSession, EngineError> {
        let prefs: SessionPrefs = self.session_prefs(connection.id).await?;
        let target = SshTarget {
            host: connection.host.clone(),
            port: connection.port,
            user: connection.username.clone().unwrap_or_default(),
        };
        let totp_prompt = prefs.totp_pattern();
        let options = SessionOptions {
            keepalive: Duration::from_secs(u64::from(prefs.keepalive_secs)),
            auth_order: prefs.auth_order,
            agents: self.agent_sources().await?,
            agent_fingerprint: prefs.agent_fingerprint,
            own_key: connection.key.as_ref().map(|key| key.fingerprint.clone()),
            max_agent_keys: self.agent_max_keys().await,
            totp_prompt,
            use_certificates: self.setting_on(crate::settings::SSH_CERTIFICATES_KEY).await,
            ..SessionOptions::default()
        };
        let gate = Arc::new(EngineGate {
            engine: self.clone(),
            hop,
        });
        let credentials = EngineCredentials {
            engine: self.clone(),
            connection: connection.id,
            key_path: connection.key_path.clone(),
            announce,
        };
        let session = match via {
            None => SshSession::connect(&target, &options, gate, &credentials, cancel).await,
            Some(via) => {
                SshSession::connect_via(via, &target, &options, gate, &credentials, cancel).await
            }
        };
        Ok(session?)
    }
}

impl EngineError {
    /// This failure of the jump host `hop`, named by it; a locked vault and a cancel stay as they
    /// are - the GUI answers them the same whichever hop met them.
    pub(crate) fn at_hop(self, hop: &HopLabel) -> Self {
        match self {
            Self::VaultLocked | Self::Cancelled | Self::Hop { .. } => self,
            other => Self::Hop {
                hop: hop.name.clone(),
                index: hop.index,
                total: hop.total,
                source: Box::new(other),
            },
        }
    }
}

#[cfg(test)]
#[path = "sessions_chain_tests.rs"]
mod tests;
