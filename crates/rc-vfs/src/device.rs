//! Portable devices - phones, tablets and cameras on USB over MTP (RC-100-61) - as drives.
//!
//! MTP is no file system: a device holds objects with ids, not paths; a file is written whole
//! with its size stated first, never in part; there are no permissions, no resumed transfers,
//! and some devices rename nothing. A device can go at any moment, or keep its files to itself
//! (Android until "File transfer" is chosen on the phone).
//!
//! - **Windows** reaches a device through Windows Portable Devices (WPD, `windows.rs`): one
//!   [`DeviceFs`] per device, its backend [`crate::BackendId::Device`], whose paths are names
//!   (`/<storage>/<folder>/<file>`, the storages as the top folders). [`DeviceFs`] resolves a path
//!   to its object along the names, keeping what it found in a bounded cache (`cache.rs`); the
//!   device's own calls are a [`Session`], so everything above them is portable and tested
//!   against a fake device (`fake.rs`). The COM objects live on one thread per device
//!   (`worker.rs`).
//! - **Linux** has gvfs mount MTP devices under `$XDG_RUNTIME_DIR/gvfs/mtp:host=...` (`gvfs.rs`):
//!   a device is that folder on the local backend, mounted with `gio mount` on first use.
//! - **macOS** lists no devices (owner, 2026-11-01).
//!
//! [`DeviceHub`] is where the drive bar gets the devices from; [`system_hub`] is this platform's.

// The path cache and the file system over a session run on Windows only and are tested
// everywhere; Linux reaches its devices through gvfs and the local backend.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the WPD session reads through the cache")
)]
mod cache;
mod codes;
#[cfg(test)]
mod fake;
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the WPD session is a DeviceFs")
)]
mod fs;
// The parsing compiles and is tested everywhere; only Linux and the BSDs ask gvfs.
#[cfg_attr(
    any(windows, target_os = "macos"),
    allow(dead_code, reason = "only Linux and the BSDs ask gvfs")
)]
mod gvfs;
mod names;
mod pipe;
#[cfg(windows)]
mod windows;
// The thread of a device's COM objects; it runs on Windows only and is tested everywhere.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the WPD session runs on a worker")
)]
mod worker;
mod write;

use std::{
    io::{self, Read},
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::SystemTime,
};

use crate::{Result, Vfs, VfsError, VfsPath};

pub use fs::DeviceFs;

/// One portable device, as the drive bar lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableDevice {
    /// Stable while it is plugged in: WPD's device id on Windows, gvfs' host on Linux.
    pub id: String,
    /// What the device calls itself ("Pixel 8").
    pub name: String,
    /// Where a panel goes: the root of its [`DeviceFs`] (Windows), its gvfs folder (Linux).
    pub root: VfsPath,
    /// The `mtp://` location gvfs mounts on first use; `None` once it is reachable.
    pub mount: Option<String>,
}

/// Where the portable devices come from: the system, or a test's stand-in.
pub trait DeviceHub: Send + Sync {
    /// The devices there now. Blocks, but asks no device for its files.
    fn devices(&self) -> Vec<PortableDevice>;

    /// Makes `device` reachable and answers the backend that serves its root, `None` when the
    /// root is on the local backend (a gvfs folder, mounted here when it was not). Blocks while
    /// the device opens.
    ///
    /// # Errors
    ///
    /// `device_unavailable` for a device that is gone, locked or not sharing its files; the
    /// typed error of the mount or the session otherwise.
    fn open(&self, device: &PortableDevice) -> Result<Option<Arc<dyn Vfs>>>;
}

/// The portable devices of this platform: WPD on Windows, gvfs on Linux, none on macOS.
#[must_use]
pub fn system_hub() -> Arc<dyn DeviceHub> {
    #[cfg(windows)]
    let hub: Arc<dyn DeviceHub> = Arc::new(windows::WpdHub::default());
    #[cfg(all(unix, not(target_os = "macos")))]
    let hub: Arc<dyn DeviceHub> = Arc::new(gvfs::GvfsHub::default());
    #[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
    let hub: Arc<dyn DeviceHub> = Arc::new(NoDevices);
    hub
}

/// A platform without portable devices.
#[cfg_attr(
    any(windows, all(unix, not(target_os = "macos"))),
    allow(dead_code, reason = "only macOS has no devices")
)]
struct NoDevices;

impl DeviceHub for NoDevices {
    fn devices(&self) -> Vec<PortableDevice> {
        Vec::new()
    }

    fn open(&self, device: &PortableDevice) -> Result<Option<Arc<dyn Vfs>>> {
        Err(VfsError::DeviceUnavailable {
            path: PathBuf::from(&device.name),
        })
    }
}

/// One object of a device: a storage, a folder or a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Object {
    /// The device's id of it.
    pub id: String,
    /// Its file name.
    pub name: String,
    /// A storage or a folder: something that holds objects.
    pub folder: bool,
    /// Bytes of a file; 0 for a folder.
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// What a device says it does with its objects, asked once when its session opens: what it
/// does not do is refused before the device is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Abilities {
    /// Takes new files.
    pub write: bool,
    /// Creates folders.
    pub mkdir: bool,
    /// Deletes objects.
    pub delete: bool,
    /// Changes an object's name.
    pub rename: bool,
}

impl Abilities {
    /// A device that does everything MTP has.
    #[cfg(test)]
    pub(crate) const ALL: Self = Self {
        write: true,
        mkdir: true,
        delete: true,
        rename: true,
    };
}

/// What went wrong on a device, before [`DeviceFs`] gives it the path.
#[derive(Debug)]
pub(crate) enum DeviceError {
    /// Unplugged, locked, or not sharing its files.
    Unavailable,
    /// The object is not there (any more).
    NotFound,
    /// Something of that name is there already.
    Exists,
    /// The device refused.
    Denied,
    /// The transfer was stopped.
    Cancelled,
    /// The device does not do this.
    Unsupported(&'static str),
    /// Anything else.
    Io(io::Error),
}

impl DeviceError {
    /// This failure on `path`, typed.
    pub(crate) fn on(self, path: impl Into<PathBuf>) -> VfsError {
        let path = path.into();
        match self {
            Self::Unavailable => VfsError::DeviceUnavailable { path },
            Self::NotFound => VfsError::NotFound { path },
            Self::Exists => VfsError::AlreadyExists { path },
            Self::Denied => VfsError::PermissionDenied { path },
            Self::Cancelled => VfsError::Cancelled { path },
            Self::Unsupported(operation) => VfsError::Unsupported { path, operation },
            Self::Io(source) => VfsError::from_io(source, path),
        }
    }
}

/// The result of a [`Session`] call.
pub(crate) type DeviceResult<T> = std::result::Result<T, DeviceError>;

/// Where a download's bytes go: `false` once nobody wants more, and the download stops.
pub(crate) type Sink = Box<dyn FnMut(&[u8]) -> bool + Send>;

/// A device's own calls, by object id. Each blocks until the device answered.
pub(crate) trait Session: Send + Sync {
    /// What the device does with its objects.
    fn abilities(&self) -> Abilities;

    /// The storages: the device's top folders.
    fn storages(&self) -> DeviceResult<Vec<Object>>;

    /// The objects in the folder or storage `folder`.
    fn children(&self, folder: &str) -> DeviceResult<Vec<Object>>;

    /// Free bytes of the storage `storage`, when the device tells.
    fn free_space(&self, storage: &str) -> DeviceResult<Option<u64>>;

    /// Sends the file `file`'s bytes to `sink`, from the start.
    fn download(&self, file: &str, sink: Sink) -> DeviceResult<()>;

    /// Creates the file `name` of `size` bytes in `folder` from `source`. Checks `cancel` between
    /// chunks; a stopped or failed upload leaves nothing on the device.
    fn upload(
        &self,
        folder: &str,
        name: &str,
        size: u64,
        source: Box<dyn Read + Send>,
        cancel: Arc<AtomicBool>,
    ) -> DeviceResult<Object>;

    /// Creates the folder `name` in `parent`.
    fn create_folder(&self, parent: &str, name: &str) -> DeviceResult<Object>;

    /// Deletes the object `id`, a folder with what it holds when `recursive`.
    fn delete(&self, id: &str, recursive: bool) -> DeviceResult<()>;

    /// Gives the object `id` the name `name`, in its folder.
    fn rename(&self, id: &str, name: &str) -> DeviceResult<()>;
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
