//! What a session connects to and how: the target, the timeouts, the keepalive and the order in
//! which the authentication methods are tried.

use std::{fmt, time::Duration};

use serde::{Deserialize, Serialize};

use crate::agent::{AgentSource, AgentSourceKind};

/// The longest the TCP connect may take.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The longest the SSH handshake (version, key exchange, host-key check) may take.
pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// The longest the authentication may take, every method together.
pub const DEFAULT_AUTH_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the server may stay silent before a keepalive is sent.
pub const DEFAULT_KEEPALIVE: Duration = Duration::from_secs(30);
/// Unanswered keepalives after which the connection counts as lost.
pub const KEEPALIVE_MAX: usize = 3;
/// The most agent keys one login offers by default: a server counts each against its
/// `MaxAuthTries` (six by default in OpenSSH) and learns every key offered (RC-080-43).
pub const DEFAULT_MAX_AGENT_KEYS: u32 = 5;
/// The highest the most agent keys offered may be set.
pub const MAX_AGENT_KEYS: u32 = 20;

/// An authentication method, as the order of a connection names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    /// The keys an SSH agent holds (ssh-agent, the Windows OpenSSH agent service, Pageant).
    Agent,
    /// The private key stored in the vault, or the key file of the connection, with its
    /// passphrase from the vault.
    Key,
    /// The password stored in the vault; over `keyboard-interactive` when the server asks for it
    /// there instead.
    Password,
}

impl AuthKind {
    /// The order a connection without its own uses: agent, key, password.
    pub const DEFAULT_ORDER: [Self; 3] = [Self::Agent, Self::Key, Self::Password];

    /// The name the order is stored and shown with.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Key => "key",
            Self::Password => "password",
        }
    }

    /// The method named `text`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::DEFAULT_ORDER
            .into_iter()
            .find(|kind| kind.as_str() == text)
    }
}

impl fmt::Display for AuthKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An order as stored: `agent,key,password`. `None` when it is empty, names an unknown method or
/// names one twice.
#[must_use]
pub fn parse_order(text: &str) -> Option<Vec<AuthKind>> {
    let mut order = Vec::new();
    for part in text.split(',') {
        let kind = AuthKind::parse(part.trim())?;
        if order.contains(&kind) {
            return None;
        }
        order.push(kind);
    }
    Some(order)
}

/// The stored form of `order`.
#[must_use]
pub fn format_order(order: &[AuthKind]) -> String {
    order
        .iter()
        .map(|kind| kind.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// The method that signed the login in: what is logged and answered, never a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum AuthUsed {
    /// A key of an agent, by its fingerprint.
    Agent {
        source: AgentSourceKind,
        fingerprint: String,
    },
    /// The connection's private key, by its fingerprint.
    Key {
        fingerprint: String,
    },
    Password,
    /// The password, answering the server's `keyboard-interactive` prompt.
    KeyboardInteractive,
}

impl AuthUsed {
    /// The method's name in a log line.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Agent { .. } => "agent",
            Self::Key { .. } => "key",
            Self::Password => "password",
            Self::KeyboardInteractive => "keyboard-interactive",
        }
    }
}

/// The server and the account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    pub host: String,
    pub port: u16,
    /// The login name; a session without one is refused before anything is sent.
    pub user: String,
}

/// How a session is set up and kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOptions {
    pub connect_timeout: Duration,
    pub handshake_timeout: Duration,
    pub auth_timeout: Duration,
    /// The keepalive interval; after [`KEEPALIVE_MAX`] unanswered ones the session is lost.
    pub keepalive: Duration,
    /// The methods to try, in this order; one without its material is skipped.
    pub auth_order: Vec<AuthKind>,
    /// The agents [`AuthKind::Agent`] asks, in this order.
    pub agents: Vec<AgentSource>,
    /// Only the agent key with this `SHA256:` fingerprint is offered; every key when `None`.
    pub agent_fingerprint: Option<String>,
    /// The `SHA256:` fingerprint of the connection's own key: offered first when an agent holds
    /// it.
    pub own_key: Option<String>,
    /// The most agent keys one login offers, every agent together.
    pub max_agent_keys: u32,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
            auth_timeout: DEFAULT_AUTH_TIMEOUT,
            keepalive: DEFAULT_KEEPALIVE,
            auth_order: AuthKind::DEFAULT_ORDER.to_vec(),
            agents: Vec::new(),
            agent_fingerprint: None,
            own_key: None,
            max_agent_keys: DEFAULT_MAX_AGENT_KEYS,
        }
    }
}

/// The step of the setup a timeout hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Connect,
    Handshake,
    Auth,
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Connect => "connect",
            Self::Handshake => "handshake",
            Self::Auth => "authentication",
        })
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
