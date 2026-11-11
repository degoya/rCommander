//! The portable devices of the drive bar (RC-100-61): phones, tablets and cameras over MTP.
//!
//! The devices come from the platform's [`DeviceHub`] (`rc_vfs::system_hub`: WPD on Windows,
//! gvfs on Linux), asked off the UI thread like the drive list: at most the catalog's timeout,
//! the last list standing in meanwhile and a late one announced as [`EngineEvent::Drives`] when
//! it differs. The drive bar reads the list every few seconds, so a device plugged in or pulled
//! out shows without a restart. Opening a device (a WPD session, a gvfs mount) waits at most
//! [`OPEN_TIMEOUT`]; a device that hangs never holds a panel longer.

use std::{
    io,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rc_vfs::{DeviceHub, PortableDevice, Vfs, VfsError};
use tokio::sync::broadcast;

use super::{Drive, DriveGroup, DriveKind, DriveState};
use crate::EngineEvent;
use crate::recover::LockOrRecover as _;
use crate::recover::LogPanic as _;

/// How long opening a device may take: a phone asked for its first session, gvfs mounting.
pub(crate) const OPEN_TIMEOUT: Duration = Duration::from_secs(30);

/// `device://<name>/<path>`: how a location on a portable device is shown and written, the name
/// the drive bar's - or the device's id where several share one (RC-110-16).
pub const DEVICE_SCHEME: &str = "device://";

/// The drive bar's id of the device `device`: `device:<its id>`.
pub(crate) fn drive_id(device: &str) -> String {
    format!("device:{device}")
}

/// No portable devices: the default of a drive source, and what a test has unless it says so.
pub(crate) struct NoDevices;

impl DeviceHub for NoDevices {
    fn devices(&self) -> Vec<PortableDevice> {
        Vec::new()
    }

    fn open(&self, device: &PortableDevice) -> rc_vfs::Result<Option<Arc<dyn Vfs>>> {
        Err(VfsError::DeviceUnavailable {
            path: PathBuf::from(&device.name),
        })
    }
}

/// The devices as last listed, and the hub to ask.
pub(crate) struct DeviceList {
    hub: Arc<dyn DeviceHub>,
    timeout: Duration,
    known: Arc<Mutex<Vec<PortableDevice>>>,
    /// A list question is open.
    listing: Arc<AtomicBool>,
    events: broadcast::Sender<EngineEvent>,
}

/// Marks the list question as open until it drops, a panic of the hub included.
struct Open(Arc<AtomicBool>);

impl Drop for Open {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl DeviceList {
    pub(crate) fn new(
        hub: Arc<dyn DeviceHub>,
        timeout: Duration,
        events: broadcast::Sender<EngineEvent>,
    ) -> Self {
        Self {
            hub,
            timeout,
            known: Arc::new(Mutex::new(Vec::new())),
            listing: Arc::new(AtomicBool::new(false)),
            events,
        }
    }

    fn known(&self) -> MutexGuard<'_, Vec<PortableDevice>> {
        self.known.lock_or_recover("devices.known")
    }

    /// The devices there now: asked unless a question is still open, waited for at most the
    /// timeout; the last list otherwise.
    pub(crate) async fn list(&self) -> Vec<PortableDevice> {
        if !self.listing.swap(true, Ordering::SeqCst) {
            let hub = Arc::clone(&self.hub);
            let known = Arc::clone(&self.known);
            let open = Open(Arc::clone(&self.listing));
            let events = self.events.clone();
            // Set once this call stops waiting: a changed list after that is announced.
            let late = Arc::new(AtomicBool::new(false));
            let late_list = Arc::clone(&late);
            let asked = tokio::task::spawn_blocking(move || {
                let _open = open;
                let devices = hub.devices();
                let changed = {
                    let mut known = known.lock_or_recover("devices.known");
                    let changed = *known != devices;
                    *known = devices;
                    changed
                };
                if changed && late_list.load(Ordering::SeqCst) {
                    // No subscriber is not an error: a headless engine may have none.
                    let _ = events.send(EngineEvent::Drives);
                }
            });
            if tokio::time::timeout(self.timeout, asked)
                .await
                .map(|joined| joined.log_panic("engine.device_list"))
                .is_err()
            {
                tracing::debug!("the portable devices did not answer in time; the last list stays");
            }
            late.store(true, Ordering::SeqCst);
        }
        self.known().clone()
    }

    /// The device of the drive bar entry `drive`, as last listed.
    pub(crate) fn find(&self, drive: &str) -> Option<PortableDevice> {
        self.known()
            .iter()
            .find(|device| drive_id(&device.id) == drive)
            .cloned()
    }

    /// The device a `device://` location names, as last listed: the one with the id `name`, else
    /// the only one called `name`; `None` for none or several.
    pub(crate) fn named(&self, name: &str) -> Option<PortableDevice> {
        let known = self.known();
        if let Some(device) = known.iter().find(|device| device.id == name) {
            return Some(device.clone());
        }
        let mut called = known.iter().filter(|device| device.name == name);
        match (called.next(), called.next()) {
            (Some(device), None) => Some(device.clone()),
            _ => None,
        }
    }

    /// Makes `device` reachable (a WPD session, a gvfs mount) on a blocking thread, within
    /// [`OPEN_TIMEOUT`]: the backend of its root, or `None` when that is the local one.
    pub(crate) async fn open(
        &self,
        device: PortableDevice,
    ) -> Result<Option<Arc<dyn Vfs>>, VfsError> {
        let hub = Arc::clone(&self.hub);
        let name = PathBuf::from(&device.name);
        let task = tokio::task::spawn_blocking(move || hub.open(&device));
        match tokio::time::timeout(OPEN_TIMEOUT, task)
            .await
            .map(|joined| joined.log_panic("engine.device_open"))
        {
            Err(_) => Err(VfsError::TimedOut {
                path: name,
                seconds: OPEN_TIMEOUT.as_secs(),
            }),
            Ok(Err(error)) => Err(VfsError::Io {
                path: name,
                source: io::Error::other(error),
            }),
            Ok(Ok(opened)) => opened,
        }
    }
}

/// The drive bar's entry of `device`: its name, the phone symbol, its root.
pub(crate) fn entry(device: &PortableDevice) -> Drive {
    Drive {
        id: drive_id(&device.id),
        group: DriveGroup::Devices,
        kind: DriveKind::Device,
        label: device.name.clone(),
        root: Some(device.root.clone()),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        folder_buttons: Vec::new(),
        favourite: false,
        remote: None,
    }
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod tests;
