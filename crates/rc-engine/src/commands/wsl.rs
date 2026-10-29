//! The `wsl.*` commands: the installed WSL distributions, for the drive bar and MCP.

use crate::{Engine, EngineError, EngineEvent, Outcome};

impl Engine {
    /// `wsl.list_distros`: the kept list, read on first use.
    pub(crate) async fn wsl_distros(&self) -> Result<Outcome, EngineError> {
        let distros = self.inner.wsl.distros().await?;
        Ok(Outcome::WslDistros { distros })
    }

    /// `wsl.refresh`: the list read again; subscribers hear of it.
    pub(crate) async fn wsl_refresh(&self) -> Result<Outcome, EngineError> {
        let distros = self.inner.wsl.refresh().await?;
        self.publish(EngineEvent::WslDistros);
        Ok(Outcome::WslDistros { distros })
    }
}

#[cfg(test)]
#[path = "wsl_tests.rs"]
mod tests;
