//! The portable devices in the drive bar and a panel changing to one (RC-100-61).
//!
//! `drives.list` lists them after the drives, with `drives.portable_devices` on (the default).
//! Changing to one opens it first - a WPD session on Windows, whose backend is registered for the
//! panels and the queue as a server's is; a gvfs mount on Linux, whose folder is on the local
//! backend. A device that shares no storage (Android until "File transfer" is chosen, a locked
//! phone) is `device_unavailable`, not an empty folder: the window says what to do on the phone.

use std::{path::PathBuf, sync::Arc};

use rc_ops::Backend;
use rc_vfs::{VfsError, VfsPath};

use crate::{Drive, Engine, EngineError, drives::devices, settings::PORTABLE_DEVICES_KEY};

impl Engine {
    /// The portable devices of the drive bar; none with `drives.portable_devices` off, when the
    /// system is not even asked.
    pub(crate) async fn device_entries(&self) -> Vec<Drive> {
        if !self.setting_on(PORTABLE_DEVICES_KEY).await {
            return Vec::new();
        }
        let found = self.inner.drives.devices().list().await;
        found.iter().map(devices::entry).collect()
    }

    /// Opens the device of the drive bar entry `drive` and answers its root, where a panel goes.
    /// A device that opens but lists no storage is `device_unavailable`.
    pub(crate) async fn device_start(&self, drive: &str) -> Result<VfsPath, EngineError> {
        let list = self.inner.drives.devices();
        let device = list.find(drive).ok_or_else(|| EngineError::DriveNotFound {
            drive: drive.to_owned(),
        })?;
        if let Some(vfs) = list.open(device.clone()).await? {
            let backend = device.root.backend.clone();
            self.inner
                .remotes
                .open()
                .insert(backend.clone(), Arc::clone(&vfs));
            self.inner.ops.register(backend, Backend::new(vfs));
        }
        let storages = self.vfs(&device.root)?.list(&device.root.path).await?;
        if storages.is_empty() {
            return Err(VfsError::DeviceUnavailable {
                path: PathBuf::from(&device.name),
            }
            .into());
        }
        Ok(device.root)
    }
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod tests;
