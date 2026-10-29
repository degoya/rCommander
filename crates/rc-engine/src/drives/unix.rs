//! The drives outside Windows: `/`, the home directory, and the mount points of `/proc/mounts`
//! (Linux) or the volumes of `/Volumes` (macOS). Enough to compile and to show something
//! sensible; rCommander ships for Windows first.

use std::{
    io,
    path::{Path, PathBuf},
};

use super::{Details, DriveKind, DriveSource, RawDrive, mounts, space};

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
            Err(_) => drives.extend(volumes()),
        }
        drives
    }

    fn details(&self, root: &Path) -> io::Result<Details> {
        Ok(Details {
            volume: None,
            space: Some(space(root)?),
        })
    }
}

/// The volumes macOS mounts under `/Volumes`. The boot volume is there too, as a link to `/`,
/// which is the first drive already; a link is no volume of its own (RC-100-44).
fn volumes() -> Vec<RawDrive> {
    let Ok(entries) = std::fs::read_dir("/Volumes") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| !kind.is_symlink()))
        .map(|entry| RawDrive {
            id: entry.path().to_string_lossy().into_owned(),
            label: entry.file_name().to_string_lossy().into_owned(),
            kind: DriveKind::Removable,
            root: entry.path(),
            remote: None,
        })
        .collect()
}
