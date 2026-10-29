//! SSH agents: OpenSSH's `ssh-agent` behind `SSH_AUTH_SOCK` (Unix), the Windows OpenSSH agent
//! service behind the named pipe [`OPENSSH_PIPE`], and Pageant (Windows).
//!
//! All three speak the same agent protocol through russh's client
//! (`russh::keys::agent::client`), which also brings the Pageant transport (the `pageant` crate)
//! on Windows; this module only picks the transport (`agent/unix.rs`, `agent/windows.rs`) and
//! holds no `unsafe`. [`list_identities`] answers what the agents hold - the agent, key type,
//! comment and fingerprint, never a key blob - within [`AGENT_WAIT`]. [`connect`] opens one agent
//! for the SSH login (RC-060-01), which signs through the [`AgentConnection`] (russh's `Signer`).
//! Nothing is stored and nothing here touches the vault: a connection with `auth_method = agent`
//! never needs the master password.

use std::{ffi::OsString, fmt, path::PathBuf, time::Duration};

use russh::keys::{
    HashAlg,
    agent::{
        AgentIdentity as HeldKey,
        client::{AgentClient, AgentStream},
    },
};
use serde::{Deserialize, Serialize};
use tokio::time::{Instant, timeout, timeout_at};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use self::unix as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as platform;

/// No SSH agent answers.
pub const AGENT_NOT_FOUND: &str = "agent_not_found";
/// An agent did not answer in time.
pub const AGENT_TIMEOUT: &str = "agent_timeout";
/// An agent answered, but not with a list of keys.
pub const AGENT_FAILED: &str = "agent_failed";

/// The longest a listing or a [`connect`] waits for the agents, all of them together.
pub const AGENT_WAIT: Duration = Duration::from_secs(1);

/// The named pipe of the Windows OpenSSH agent service (`ssh-agent`).
pub const OPENSSH_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

/// The setting that picks the agents on Windows, a [`WindowsAgents`].
pub const AGENT_SOURCES_KEY: &str = "ssh.agent_sources";

/// Where an agent listens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSource {
    /// A Unix socket, the path in `SSH_AUTH_SOCK`.
    UnixSocket(PathBuf),
    /// The Windows OpenSSH agent service, [`OPENSSH_PIPE`].
    OpenSshPipe,
    /// Pageant (PuTTY), over its named pipe or its window.
    Pageant,
}

impl AgentSource {
    /// The kind, as answers name it.
    #[must_use]
    pub fn kind(&self) -> AgentSourceKind {
        match self {
            Self::UnixSocket(_) => AgentSourceKind::SshAuthSock,
            Self::OpenSshPipe => AgentSourceKind::OpensshPipe,
            Self::Pageant => AgentSourceKind::Pageant,
        }
    }

    /// The source for a message: its kind, and the socket's path.
    fn describe(&self) -> String {
        match self {
            Self::UnixSocket(path) => format!("{} at {}", self.kind(), path.display()),
            Self::OpenSshPipe => format!("{} at {OPENSSH_PIPE}", self.kind()),
            Self::Pageant => self.kind().to_string(),
        }
    }
}

/// The kind of an [`AgentSource`], without the socket path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSourceKind {
    SshAuthSock,
    OpensshPipe,
    Pageant,
}

impl fmt::Display for AgentSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SshAuthSock => "SSH agent (SSH_AUTH_SOCK)",
            Self::OpensshPipe => "OpenSSH agent service",
            Self::Pageant => "Pageant",
        })
    }
}

/// Which agents rCommander asks on Windows, in this order; the setting [`AGENT_SOURCES_KEY`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowsAgents {
    /// The OpenSSH agent service first, then Pageant.
    #[default]
    Both,
    Openssh,
    Pageant,
}

/// A key an agent holds, as far as anyone outside the SSH login sees it: never the key itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentIdentity {
    /// The agent holding it.
    pub source: AgentSourceKind,
    /// The key type as SSH names it (`ssh-ed25519`, `ssh-rsa`, ...; a certificate's
    /// `...-cert-v01@openssh.com`).
    pub algorithm: String,
    /// `SHA256:<base64>` exactly as `ssh-add -l` prints it.
    pub fingerprint_sha256: String,
    /// The comment the key was added with, often its file or `user@host`.
    pub comment: String,
}

impl AgentIdentity {
    fn of(source: AgentSourceKind, held: &HeldKey) -> Self {
        let algorithm = match held {
            HeldKey::PublicKey { key, .. } => key.algorithm().as_str().to_owned(),
            HeldKey::Certificate { certificate, .. } => {
                certificate.algorithm().to_certificate_type()
            }
        };
        Self {
            source,
            algorithm,
            fingerprint_sha256: held.public_key().fingerprint(HashAlg::Sha256).to_string(),
            comment: held.comment().to_owned(),
        }
    }
}

/// What the agents hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentListing {
    /// The agents that answered, in the order asked.
    pub sources: Vec<AgentSourceKind>,
    /// Their keys; a key two agents hold is listed once, for the first.
    pub identities: Vec<AgentIdentity>,
}

/// Why no agent answered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentError {
    /// No agent runs, or none is configured: a plain answer, not a failure.
    #[error("no SSH agent found: {detail}")]
    NotFound { detail: String },
    /// An agent took longer than [`AGENT_WAIT`].
    #[error("the {agent} did not answer within a second")]
    Timeout { agent: AgentSourceKind },
    /// An agent answered, but not with its keys.
    #[error("the {agent} failed: {reason}")]
    Failed {
        agent: AgentSourceKind,
        reason: String,
    },
}

impl AgentError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[AGENT_NOT_FOUND, AGENT_TIMEOUT, AGENT_FAILED];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => AGENT_NOT_FOUND,
            Self::Timeout { .. } => AGENT_TIMEOUT,
            Self::Failed { .. } => AGENT_FAILED,
        }
    }
}

/// An open agent. It signs the SSH login through russh's `Signer`, so its keys never leave it.
pub type AgentConnection = AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>;

/// The agents to ask on this machine: on Windows those `wanted` names, elsewhere the socket in
/// `SSH_AUTH_SOCK` (none when it is not set).
#[must_use]
pub fn sources(wanted: WindowsAgents) -> Vec<AgentSource> {
    sources_for(cfg!(windows), wanted, std::env::var_os("SSH_AUTH_SOCK"))
}

fn sources_for(
    on_windows: bool,
    wanted: WindowsAgents,
    auth_sock: Option<OsString>,
) -> Vec<AgentSource> {
    if on_windows {
        return match wanted {
            WindowsAgents::Both => vec![AgentSource::OpenSshPipe, AgentSource::Pageant],
            WindowsAgents::Openssh => vec![AgentSource::OpenSshPipe],
            WindowsAgents::Pageant => vec![AgentSource::Pageant],
        };
    }
    auth_sock
        .filter(|path| !path.is_empty())
        .map(|path| vec![AgentSource::UnixSocket(PathBuf::from(path))])
        .unwrap_or_default()
}

/// The keys `sources` hold, asked in order, within [`AGENT_WAIT`] for all of them.
///
/// One agent answering is enough; one that fails beside it is logged and left out.
///
/// # Errors
///
/// None answered: [`AgentError::NotFound`] when none runs, otherwise the first failure.
pub async fn list_identities(sources: &[AgentSource]) -> Result<AgentListing, AgentError> {
    list_within(sources, AGENT_WAIT).await
}

async fn list_within(sources: &[AgentSource], wait: Duration) -> Result<AgentListing, AgentError> {
    if sources.is_empty() {
        return Err(AgentError::NotFound {
            detail: "SSH_AUTH_SOCK is not set".to_owned(),
        });
    }
    let deadline = Instant::now() + wait;
    let mut listing = AgentListing::default();
    let mut absent = Vec::new();
    let mut failures = Vec::new();
    for source in sources {
        match timeout_at(deadline, ask(source)).await {
            Ok(Ok(identities)) => {
                listing.sources.push(source.kind());
                for identity in identities {
                    let known = listing
                        .identities
                        .iter()
                        .any(|known| known.fingerprint_sha256 == identity.fingerprint_sha256);
                    if !known {
                        listing.identities.push(identity);
                    }
                }
            }
            Ok(Err(Asked::Absent(detail))) => {
                absent.push(format!("{}: {detail}", source.describe()));
            }
            Ok(Err(Asked::Failed(reason))) => failures.push(AgentError::Failed {
                agent: source.kind(),
                reason,
            }),
            Err(_) => failures.push(AgentError::Timeout {
                agent: source.kind(),
            }),
        }
    }
    if !listing.sources.is_empty() {
        for failure in &failures {
            tracing::warn!(%failure, "an SSH agent failed; the others answered");
        }
        return Ok(listing);
    }
    Err(failures
        .into_iter()
        .next()
        .unwrap_or_else(|| AgentError::NotFound {
            detail: absent.join("; "),
        }))
}

/// Why one agent gave no keys.
enum Asked {
    /// Nothing to connect to.
    Absent(String),
    /// Connected, but the answer was not a list of keys.
    Failed(String),
}

async fn ask(source: &AgentSource) -> Result<Vec<AgentIdentity>, Asked> {
    let mut agent = open(source)
        .await
        .map_err(|error| Asked::Absent(error.to_string()))?;
    let held = agent
        .request_identities()
        .await
        .map_err(|error| Asked::Failed(error.to_string()))?;
    Ok(held
        .iter()
        .map(|held| AgentIdentity::of(source.kind(), held))
        .collect())
}

/// Opens `source` for the SSH login, within [`AGENT_WAIT`].
///
/// # Errors
///
/// [`AgentError::NotFound`] when nothing listens there, [`AgentError::Timeout`] when it does not
/// accept in time.
pub async fn connect(source: &AgentSource) -> Result<AgentConnection, AgentError> {
    match timeout(AGENT_WAIT, open(source)).await {
        Ok(Ok(agent)) => Ok(agent),
        Ok(Err(error)) => Err(AgentError::NotFound {
            detail: format!("{}: {error}", source.describe()),
        }),
        Err(_) => Err(AgentError::Timeout {
            agent: source.kind(),
        }),
    }
}

async fn open(source: &AgentSource) -> Result<AgentConnection, russh::keys::Error> {
    match source {
        AgentSource::UnixSocket(path) => platform::connect_socket(path).await,
        AgentSource::OpenSshPipe => platform::connect_pipe(OPENSSH_PIPE).await,
        AgentSource::Pageant => platform::connect_pageant().await,
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
