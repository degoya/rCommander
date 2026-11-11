//! The portable devices in the drive bar and a panel changing to one (RC-100-61).
//!
//! `drives.list` lists them after the drives, with `drives.portable_devices` on (the default).
//! Changing to one opens it first - a WPD session on Windows, whose backend is registered for the
//! panels and the queue as a server's is; a gvfs mount on Linux, whose folder is on the local
//! backend. A device that shares no storage (Android until "File transfer" is chosen, a locked
//! phone) is `device_unavailable`, not an empty folder: the window says what to do on the phone.
//! A device's location is written `device://<name>/<path>` (RC-110-16), so a target typed or
//! offered in a dialog keeps its device instead of being read against the other panel's drive.

use std::{
    path::{Component, PathBuf},
    sync::Arc,
};

use rc_ops::Backend;
use rc_vfs::{BackendId, VfsError, VfsPath};

use crate::{
    Drive, Engine, EngineError, drives::devices, remote::split_location,
    settings::PORTABLE_DEVICES_KEY,
};

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

    /// `device://<name>/<path>` (`rest`, without the scheme): the place below the root of the
    /// device `name` names ([`devices::DeviceList::named`]), opened first unless a session of it
    /// is registered already (RC-110-16). Without the path it is the device's root.
    ///
    /// # Errors
    ///
    /// `drives.portable_devices` is off, or no device or several have the name
    /// (`drive_not_found`); what [`Engine::device_start`] fails with.
    pub(crate) async fn locate_device(&self, rest: &str) -> Result<VfsPath, EngineError> {
        let (name, below) = split_location(rest);
        let not_found = || EngineError::DriveNotFound {
            drive: name.to_owned(),
        };
        if !self.setting_on(PORTABLE_DEVICES_KEY).await {
            return Err(not_found());
        }
        let list = self.inner.drives.devices();
        list.list().await;
        let device = list.named(name).ok_or_else(not_found)?;
        let open = matches!(device.root.backend, BackendId::Device { .. })
            && self.vfs(&device.root).is_ok();
        let mut located = if open {
            device.root
        } else {
            self.device_start(&devices::drive_id(&device.id)).await?
        };
        let names = below.iter().flat_map(|path| path.components());
        located
            .path
            .extend(names.filter(|part| matches!(part, Component::Normal(_))));
        Ok(located)
    }
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod tests;
