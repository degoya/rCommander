//! `drives.map_network` and `drives.unmap_network`: a share mapped to a drive letter and
//! disconnected again (RC-010-16), through Windows' own functions (`drives/network.rs`); on macOS
//! a share mounted under `/Volumes` and unmounted again (RC-110-40).
//!
//! Mapping is `GuiOnly`: the system's credential dialog needs a person, and MCP never gets to
//! start it. Disconnecting is `Destructive` - a forced disconnect closes open files - so MCP needs
//! the person's confirmation. Both check what they are given before the system sees it, run the
//! blocking call on the blocking pool, and answer the drive bar as it is afterwards, announced as
//! [`EngineEvent::Drives`].

use std::{path::PathBuf, sync::Arc};

use rc_vfs::{BackendId, VfsPath};

use crate::recover::LogPanic as _;
use crate::{
    Drive, DriveGroup, DriveKind, Engine, EngineError, EngineEvent, Outcome,
    drives::{
        self, DriveSource,
        network::{self, NetworkDriveError},
        share,
    },
};

impl Engine {
    /// `drives.map_network`: `remote` (`\\server\share[\...]` or `smb://server/share`) on the
    /// free drive `letter`; `reconnect` keeps it across sign-ins. Where shares are mounted
    /// (macOS) the system chooses the mount point, and `letter` and `reconnect` are not used.
    pub(crate) async fn map_network_drive(
        &self,
        remote: &str,
        letter: &str,
        reconnect: bool,
    ) -> Result<Outcome, EngineError> {
        let remote = network::parse_remote(remote)?;
        let source = self.inner.drives.source();
        if source.mounts_shares() {
            return self.mount_share(source, &remote).await;
        }
        let listed = self.inner.drives.list(false).await;
        let letter = network::parse_letter(letter, &listed)?;
        let folder = remote.clone();
        let answer =
            tokio::task::spawn_blocking(move || source.map_network(letter, &folder, reconnect))
                .await
                .log_panic("engine.map_network_drive")
                .map_err(|_| NetworkDriveError::Failed { code: 0 })?;
        answer.map_err(|code| network::from_system(code, letter, &remote))?;
        Ok(self.drives_changed().await)
    }

    /// Mounts the share `remote` (`\\server\share`) through the system (macOS).
    async fn mount_share(
        &self,
        source: Arc<dyn DriveSource>,
        remote: &str,
    ) -> Result<Outcome, EngineError> {
        let url = share::url(remote)?;
        let answer = tokio::task::spawn_blocking(move || source.mount_share(&url))
            .await
            .log_panic("engine.mount_share")
            .map_err(|_| NetworkDriveError::Failed { code: 0 })?;
        answer.map_err(|code| network::from_mount(code, &share::display(remote)))?;
        Ok(self.drives_changed().await)
    }

    /// `drives.unmap_network`: disconnects the network drive `drive` (a letter, or on macOS the
    /// mount point or the volume's name) and forgets it for the next sign-in; with open files
    /// only with `force`.
    pub(crate) async fn unmap_network_drive(
        &self,
        drive: &str,
        force: bool,
    ) -> Result<Outcome, EngineError> {
        let listed = self.inner.drives.list(false).await;
        let found = drives::find(&listed, drive).ok_or_else(|| EngineError::DriveNotFound {
            drive: drive.to_owned(),
        })?;
        if found.group != DriveGroup::Drives || found.kind != DriveKind::Network {
            return Err(not_network(found));
        }
        let source = self.inner.drives.source();
        if source.mounts_shares() {
            let point = mount_point(found).ok_or_else(|| not_network(found))?;
            let label = found.label.clone();
            let answer = tokio::task::spawn_blocking(move || source.unmount_share(&point, force))
                .await
                .log_panic("engine.unmount_share")
                .map_err(|_| NetworkDriveError::Failed { code: 0 })?;
            answer.map_err(|errno| network::from_unmount(errno, &label))?;
            return Ok(self.drives_changed().await);
        }
        let letter = found.id.chars().next().ok_or_else(|| not_network(found))?;
        let remote = found.remote.clone().unwrap_or_default();
        let answer = tokio::task::spawn_blocking(move || source.unmap_network(letter, force))
            .await
            .log_panic("engine.unmap_network_drive")
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

/// `drive` refused as no network drive.
fn not_network(drive: &Drive) -> EngineError {
    NetworkDriveError::NotNetwork {
        drive: drive.label.clone(),
    }
    .into()
}

/// Where the share `drive` is mounted: its local root.
fn mount_point(drive: &Drive) -> Option<PathBuf> {
    match &drive.root {
        Some(VfsPath {
            backend: BackendId::Local,
            path,
        }) => Some(path.clone()),
        _ => None,
    }
}

#[cfg(test)]
#[path = "network_drives_tests.rs"]
mod tests;
