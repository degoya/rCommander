//! The drives outside Windows: `/`, the home directory, and the mount points of `/proc/mounts`
//! (Linux) or the volumes of `/Volumes` (macOS). Enough to compile and to show something
//! sensible; rCommander ships for Windows first.
//!
//! On macOS a volume of the mount table's network file systems is a network drive with its share
//! (`drives/share.rs`), and a share is mounted and unmounted through the system
//! (`drives/macos.rs`, RC-110-40).

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_vfs::DeviceHub;

#[cfg(target_os = "macos")]
use super::macos;
use super::{
    Details, DriveKind, DriveSource, RawDrive, mounts,
    share::{self, MountEntry},
    space,
};

pub(super) struct System;

impl DriveSource for System {
    fn drives(&self) -> Vec<RawDrive> {
        let mut drives = vec![RawDrive {
            id: "/".to_owned(),
            label: "/".to_owned(),
            kind: DriveKind::Fixed,
            root: PathBuf::from("/"),
            remote: None,
        }];
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
            && home.is_absolute()
            && home != Path::new("/")
        {
            drives.push(RawDrive {
                id: home.to_string_lossy().into_owned(),
                label: "~".to_owned(),
                kind: DriveKind::Fixed,
                root: home,
                remote: None,
            });
        }
        match std::fs::read_to_string("/proc/mounts") {
            Ok(text) => drives.extend(mounts::parse(&text)),
            Err(_) => drives.extend(volumes(&mount_table())),
        }
        drives
    }

    fn details(&self, root: &Path) -> io::Result<Details> {
        Ok(Details {
            volume: None,
            space: Some(space(root)?),
        })
    }

    /// The phones and cameras gvfs mounts (Linux; none on macOS), RC-100-61.
    fn device_hub(&self) -> Arc<dyn DeviceHub> {
        rc_vfs::system_hub()
    }

    #[cfg(target_os = "macos")]
    fn mounts_shares(&self) -> bool {
        true
    }

    #[cfg(target_os = "macos")]
    fn mount_share(&self, url: &str) -> Result<(), i32> {
        macos::mount(url)
    }

    #[cfg(target_os = "macos")]
    fn unmount_share(&self, point: &Path, force: bool) -> Result<(), i32> {
        macos::unmount(point, force)
    }
}

/// The system's mount table: macOS's.
#[cfg(target_os = "macos")]
fn mount_table() -> Vec<MountEntry> {
    macos::mount_table()
}

/// None outside macOS, where `/proc/mounts` tells the kinds.
#[cfg(not(target_os = "macos"))]
fn mount_table() -> Vec<MountEntry> {
    Vec::new()
}

/// The volumes macOS mounts under `/Volumes`. The boot volume is there too, as a link to `/`,
/// which is the first drive already; a link is no volume of its own (RC-100-44). A volume the
/// mount `table` has as a network file system is a network drive with its share (RC-110-40).
fn volumes(table: &[MountEntry]) -> Vec<RawDrive> {
    let Ok(entries) = std::fs::read_dir("/Volumes") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| !kind.is_symlink()))
        .map(|entry| {
            let root = entry.path();
            let remote = share::remote_at(&root, table);
            RawDrive {
                id: root.to_string_lossy().into_owned(),
                label: entry.file_name().to_string_lossy().into_owned(),
                kind: if remote.is_some() {
                    DriveKind::Network
                } else {
                    DriveKind::Removable
                },
                root,
                remote,
            }
        })
        .collect()
}
