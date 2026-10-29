//! `drives.map_network` and `drives.unmap_network`: a share mapped to a drive letter and
//! disconnected again (RC-010-16), through Windows' own functions (`drives/network.rs`).
//!
//! Mapping is `GuiOnly`: Windows' credential dialog needs a person, and MCP never gets to start
//! it. Disconnecting is `Destructive` - a forced disconnect closes open files - so MCP needs the
//! person's confirmation. Both check what they are given before Windows sees it, run the
//! blocking call on the blocking pool, and answer the drive bar as it is afterwards, announced as
//! [`EngineEvent::Drives`].

use crate::{
    DriveGroup, DriveKind, Engine, EngineError, EngineEvent, Outcome,
    drives::{
        self,
        network::{self, NetworkDriveError},
    },
};

impl Engine {
    /// `drives.map_network`: `remote` (`\\server\share[\...]`) on the free drive `letter`;
    /// `reconnect` keeps it across sign-ins.
    pub(crate) async fn map_network_drive(
        &self,
        remote: &str,
        letter: &str,
        reconnect: bool,
    ) -> Result<Outcome, EngineError> {
        let remote = network::parse_remote(remote)?;
        let listed = self.inner.drives.list(false).await;
        let letter = network::parse_letter(letter, &listed)?;
        let source = self.inner.drives.source();
        let folder = remote.clone();
        let answer =
            tokio::task::spawn_blocking(move || source.map_network(letter, &folder, reconnect))
                .await
                .map_err(|_| NetworkDriveError::Failed { code: 0 })?;
        answer.map_err(|code| network::from_system(code, letter, &remote))?;
        Ok(self.drives_changed().await)
    }

    /// `drives.unmap_network`: disconnects the network drive `drive` and forgets it for the
    /// next sign-in; with open files only with `force`.
    pub(crate) async fn unmap_network_drive(
        &self,
        drive: &str,
        force: bool,
    ) -> Result<Outcome, EngineError> {
        let listed = self.inner.drives.list(false).await;
        let found = drives::find(&listed, drive).ok_or_else(|| EngineError::DriveNotFound {
            drive: drive.to_owned(),
        })?;
        let letter = match found.id.chars().next() {
            Some(letter)
                if found.group == DriveGroup::Drives && found.kind == DriveKind::Network =>
            {
                letter
            }
            _ => {
                return Err(NetworkDriveError::NotNetwork {
                    drive: found.label.clone(),
                }
                .into());
            }
        };
        let remote = found.remote.clone().unwrap_or_default();
        let source = self.inner.drives.source();
        let answer = tokio::task::spawn_blocking(move || source.unmap_network(letter, force))
            .await
            .map_err(|_| NetworkDriveError::Failed { code: 0 })?;
        answer.map_err(|code| network::from_system(code, letter, &remote))?;
        Ok(self.drives_changed().await)
    }

    /// The drive bar after a drive came or went, announced to the window.
    async fn drives_changed(&self) -> Outcome {
        let outcome = self.list_drives(false).await;
        self.publish(EngineEvent::Drives);
        outcome
    }
}

#[cfg(test)]
#[path = "network_drives_tests.rs"]
mod tests;
