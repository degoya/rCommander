//! The `agent.*` command: what the running SSH agents hold.
//!
//! The agents come from the platform (`SSH_AUTH_SOCK` on Unix; on Windows the OpenSSH agent
//! service and Pageant, in the order of the setting `ssh.agent_sources`). The answer names the
//! agent, key type, comment and fingerprint of each key - never a key blob - and nothing here
//! reads a secret or the vault, so a connection that authenticates with an agent never asks for
//! the master password.

use rc_remote::agent::{self, AGENT_SOURCES_KEY, AgentSource, WindowsAgents};

use crate::{Engine, EngineError, Outcome};

impl Engine {
    /// Fixes where `agent.list_identities` looks, in place of the platform's agents and the
    /// setting; for tests and a host that knows its agent. Answers `false`, changing nothing,
    /// when they are fixed already.
    pub fn attach_agent_sources(&self, sources: Vec<AgentSource>) -> bool {
        self.inner.agent_sources.set(sources).is_ok()
    }

    /// `agent.list_identities`.
    pub(crate) async fn list_agent_identities(&self) -> Result<Outcome, EngineError> {
        let sources = self.agent_sources().await?;
        let listing = agent::list_identities(&sources).await?;
        Ok(Outcome::AgentIdentities {
            sources: listing.sources,
            identities: listing.identities,
        })
    }

    /// The agents to ask, in order: the fixed ones, or the platform's and the setting's. The
    /// SSH login (`connections.connect`) asks the same.
    pub(crate) async fn agent_sources(&self) -> Result<Vec<AgentSource>, EngineError> {
        Ok(match self.inner.agent_sources.get() {
            Some(sources) => sources.clone(),
            None => agent::sources(self.windows_agents().await?),
        })
    }

    /// The setting `ssh.agent_sources`; its default without a database.
    async fn windows_agents(&self) -> Result<WindowsAgents, EngineError> {
        match self.inner.db.get() {
            Some(db) => Ok(db.get(AGENT_SOURCES_KEY).await?.unwrap_or_default()),
            None => Ok(WindowsAgents::default()),
        }
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
