//! The drive letters of Windows.
//!
//! `GetLogicalDrives` and `GetDriveTypeW` read the letters and their kinds without touching a
//! drive, so listing them is cheap even with a disconnected network share. The volume name
//! (`GetVolumeInformationW`) and the space (`fs4`, `GetDiskFreeSpaceExW`) do touch the drive and
//! are only asked through the catalog, off the UI thread and with a time limit. The calls go
//! through `winsafe`, whose safe wrappers keep `unsafe` out of rCommander; the network drives'
//! folders, mapping and disconnecting are `WNet` calls winsafe does not have (`wnet.rs`).

use std::{
    io,
    path::{Path, PathBuf},
};

use winsafe::{self as w, co};

use super::{Details, DriveKind, DriveSource, RawDrive, space, wnet};

pub(super) struct System;

impl DriveSource for System {
    fn drives(&self) -> Vec<RawDrive> {
        let present = w::GetLogicalDrives();
        let mut drives: Vec<RawDrive> = (0u8..26)
            .filter(|bit| present & (1 << bit) != 0)
            .map(|bit| {
                let letter = char::from(b'A' + bit);
                let root = format!("{letter}:\\");
                let mut kind = kind_of(w::GetDriveType(Some(&root)));
                // A remembered drive that is not connected may not tell its kind.
                let remote = if matches!(kind, DriveKind::Network | DriveKind::Unknown) {
                    wnet::remote_of(letter)
                } else {
                    None
                };
                if remote.is_some() {
                    kind = DriveKind::Network;
                }
                raw_drive(letter, kind, remote)
            })
            .collect();
        // A remembered drive not yet connected since sign-in has no letter yet (`wnet.rs`).
        for (letter, remote) in wnet::remembered() {
            if !drives.iter().any(|drive| drive.id.starts_with(letter)) {
                drives.push(raw_drive(letter, DriveKind::Network, Some(remote)));
            }
        }
        drives.sort_by(|left, right| left.id.cmp(&right.id));
        drives
    }

    fn details(&self, root: &Path) -> io::Result<Details> {
        let space = space(root)?;
        let mut name = String::new();
        let volume = w::GetVolumeInformation(
            Some(&root.to_string_lossy()),
            Some(&mut name),
            None,
            None,
            None,
            None,
        )
        .ok()
        .map(|()| name)
        .filter(|name| !name.is_empty());
        Ok(Details {
            volume,
            space: Some(space),
        })
    }

    fn map_network(&self, letter: char, remote: &str, reconnect: bool) -> Result<(), u32> {
        wnet::map(letter, remote, reconnect)
    }

    fn unmap_network(&self, letter: char, force: bool) -> Result<(), u32> {
        wnet::unmap(letter, force)
    }

    fn restore_network(&self, letter: char) -> Result<(), u32> {
        wnet::restore(letter)
    }
}

fn raw_drive(letter: char, kind: DriveKind, remote: Option<String>) -> RawDrive {
    RawDrive {
        id: format!("{letter}:"),
        label: format!("{letter}:"),
        kind,
        root: PathBuf::from(format!("{letter}:\\")),
        remote,
    }
}

fn kind_of(drive: co::DRIVE) -> DriveKind {
    if drive == co::DRIVE::FIXED {
        DriveKind::Fixed
    } else if drive == co::DRIVE::REMOVABLE {
        DriveKind::Removable
    } else if drive == co::DRIVE::REMOTE {
        DriveKind::Network
    } else if drive == co::DRIVE::CDROM {
        DriveKind::Optical
    } else if drive == co::DRIVE::RAMDISK {
        DriveKind::Ram
    } else {
        DriveKind::Unknown
    }
}
