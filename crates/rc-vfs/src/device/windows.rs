//! The portable devices of Windows: Windows Portable Devices (WPD) through the `windows`
//! crate's COM interfaces (RC-100-61).
//!
//! The device manager (`IPortableDeviceManager`) lists the devices and their names; it lives on
//! a thread of its own, made on first use and made again when it failed. Each opened device is a
//! [`WpdSession`] whose COM objects live on its own thread (`worker.rs`), behind a [`DeviceFs`]
//! that does the rest portably. Windows lists USB sticks and card readers through WPD too: those
//! have a drive letter in the drive bar already and are left out (`codes::is_mass_storage`).

mod com;
mod manager;
mod objects;
mod session;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use self::{manager::Manager, session::WpdSession};
use super::{DeviceFs, DeviceHub, PortableDevice, worker::Worker};
use crate::recover::LockOrRecover as _;
use crate::{BackendId, Result, Vfs, VfsPath};

/// The devices WPD lists, through a manager made on first use.
#[derive(Default)]
pub(crate) struct WpdHub {
    manager: Mutex<Option<Worker<Manager>>>,
}

impl DeviceHub for WpdHub {
    fn devices(&self) -> Vec<PortableDevice> {
        let mut manager = self.manager.lock_or_recover("windows.manager");
        if manager.is_none() {
            match Worker::start("rc-wpd-devices", Manager::open) {
                Ok(started) => *manager = Some(started),
                Err(error) => {
                    tracing::debug!(error = ?error, "the WPD device manager does not start");
                    return Vec::new();
                }
            }
        }
        let Some(worker) = manager.as_ref() else {
            return Vec::new();
        };
        match worker.run(|manager| manager.devices()) {
            Ok(devices) => devices
                .into_iter()
                .map(|(id, name)| PortableDevice {
                    root: VfsPath {
                        backend: BackendId::Device { device: id.clone() },
                        path: PathBuf::from("/"),
                    },
                    id,
                    name,
                    mount: None,
                })
                .collect(),
            Err(error) => {
                // Made again on the next list.
                tracing::debug!(error = ?error, "the WPD device manager stopped");
                *manager = None;
                Vec::new()
            }
        }
    }

    fn open(&self, device: &PortableDevice) -> Result<Option<Arc<dyn Vfs>>> {
        let session = WpdSession::open(&device.id).map_err(|error| error.on(&device.name))?;
        Ok(Some(Arc::new(DeviceFs::new(Arc::new(session)))))
    }
}
