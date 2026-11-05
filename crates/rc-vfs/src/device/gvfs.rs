//! The portable devices of a Linux desktop, through gvfs (GNOME, and KDE with gvfs installed).
//!
//! gvfs mounts an MTP device as a folder of its FUSE daemon, `$XDG_RUNTIME_DIR/gvfs/mtp:host=
//! <host>`, which the local backend reads and writes like any other: no dependency of ours. A
//! device plugged in but not mounted yet is one of the volumes `gio mount -li` lists, with an
//! `activation_root=mtp://<host>/`; `gio mount <that root>` mounts it, on first use. The volume's
//! name ("Pixel 8") is the device's name; a mounted device gio does not list is named by its host.
//! `gio` is asked at most every [`GIO_MAX_AGE`], the mount folder every time - reading it touches
//! no device.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::{DeviceHub, PortableDevice};
use crate::recover::LockOrRecover as _;
use crate::{Result, Vfs, VfsError, VfsPath};

/// How a gvfs mount folder of an MTP device begins.
const MTP_FOLDER: &str = "mtp:host=";

/// How long the volumes gio listed stand.
pub(crate) const GIO_MAX_AGE: Duration = Duration::from_secs(10);

/// A volume gio knows: its name and its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Volume {
    pub name: String,
    pub host: String,
}

/// The host of an `mtp://<host>/` location.
fn host_of(uri: &str) -> Option<&str> {
    let host = uri.strip_prefix("mtp://")?.trim_end_matches('/');
    (!host.is_empty() && !host.contains('/')).then_some(host)
}

/// The MTP volumes and mounts of `gio mount -li`'s output, each host once, the name of the
/// first entry that has it.
pub(crate) fn parse_gio(text: &str) -> Vec<Volume> {
    let mut volumes: Vec<Volume> = Vec::new();
    let mut name: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        let (head, rest) = line.split_once("): ").unwrap_or(("", ""));
        let uri = if head.starts_with("Volume(") {
            name = Some(rest.trim().to_owned());
            None
        } else if head.starts_with("Mount(") {
            // `Mount(0): Pixel 8 -> mtp://Google_Pixel_8_XYZ/`
            let (label, uri) = rest.rsplit_once(" -> ").unwrap_or((rest, ""));
            name = Some(label.trim().to_owned());
            Some(uri.trim())
        } else if head.starts_with("Drive(") {
            name = None;
            None
        } else {
            line.strip_prefix("activation_root=")
                .or_else(|| line.strip_prefix("default_location="))
        };
        let (Some(host), Some(label)) = (uri.and_then(host_of), name.as_ref()) else {
            continue;
        };
        if !volumes.iter().any(|volume| volume.host == host) {
            volumes.push(Volume {
                name: label.clone(),
                host: host.to_owned(),
            });
        }
    }
    volumes
}

/// The hosts of the MTP devices mounted under the gvfs folder, from its entries' names.
pub(crate) fn mounted_hosts<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    names
        .into_iter()
        .filter_map(|name| name.strip_prefix(MTP_FOLDER))
        .filter(|host| !host.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The devices: every mounted one, then every volume gio knows that is not mounted. A mounted
/// one gio does not name is named by its host (`Google_Pixel_8` as `Google Pixel 8`).
pub(crate) fn devices(gvfs: &Path, mounted: &[String], volumes: &[Volume]) -> Vec<PortableDevice> {
    let device = |host: &str, name: String, mount: Option<String>| PortableDevice {
        id: format!("mtp:{host}"),
        name,
        root: VfsPath::local(gvfs.join(format!("{MTP_FOLDER}{host}"))),
        mount,
    };
    let named = |host: &str| {
        volumes
            .iter()
            .find(|volume| volume.host == host)
            .map_or_else(|| host.replace('_', " "), |volume| volume.name.clone())
    };
    let mut devices: Vec<PortableDevice> = mounted
        .iter()
        .map(|host| device(host, named(host), None))
        .collect();
    devices.extend(
        volumes
            .iter()
            .filter(|volume| !mounted.contains(&volume.host))
            .map(|volume| {
                let mount = Some(format!("mtp://{}/", volume.host));
                device(&volume.host, volume.name.clone(), mount)
            }),
    );
    devices
}

/// The gvfs folder of this session: `$XDG_RUNTIME_DIR/gvfs`.
fn gvfs_folder() -> Option<PathBuf> {
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    runtime.is_absolute().then(|| runtime.join("gvfs"))
}

/// The volumes gio listed last, with when it was asked.
type Listed = Arc<Mutex<Option<(Instant, Vec<Volume>)>>>;

/// The devices gvfs mounts, with the volumes gio listed last.
#[derive(Default)]
pub(crate) struct GvfsHub {
    volumes: Listed,
}

impl GvfsHub {
    /// The volumes gio lists, asked again once [`GIO_MAX_AGE`] has passed. No gio, or one that
    /// fails, is no volume.
    fn volumes(&self) -> Vec<Volume> {
        let mut kept = self.volumes.lock_or_recover("gvfs.volumes");
        if let Some((asked, volumes)) = kept.as_ref()
            && asked.elapsed() < GIO_MAX_AGE
        {
            return volumes.clone();
        }
        let volumes = gio(&["mount", "-li"]).map_or_else(
            |error| {
                tracing::debug!(%error, "gio lists no volumes");
                Vec::new()
            },
            |text| parse_gio(&text),
        );
        *kept = Some((Instant::now(), volumes.clone()));
        volumes
    }
}

/// Where `gio` is looked for: system folders only, never a lookup through `PATH` (RC-080-65), so
/// a `gio` placed earlier on the `PATH` is never run.
const GIO_PATHS: &[&str] = &["/usr/bin/gio", "/bin/gio", "/usr/local/bin/gio"];

/// What `gio <args>` writes, when it succeeds.
fn gio(args: &[&str]) -> std::io::Result<String> {
    use rc_process::NoConsoleWindow;
    let program = GIO_PATHS
        .iter()
        .map(std::path::Path::new)
        .find(|path| path.is_file())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("gio is not in {}", GIO_PATHS.join(", ")),
            )
        })?;
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .no_console_window()
        .output()?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::other(said.trim().to_owned()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

impl DeviceHub for GvfsHub {
    fn devices(&self) -> Vec<PortableDevice> {
        let Some(gvfs) = gvfs_folder() else {
            return Vec::new();
        };
        let names: Vec<String> = std::fs::read_dir(&gvfs)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mounted = mounted_hosts(names.iter().map(String::as_str));
        devices(&gvfs, &mounted, &self.volumes())
    }

    fn open(&self, device: &PortableDevice) -> Result<Option<Arc<dyn Vfs>>> {
        let Some(uri) = &device.mount else {
            return Ok(None);
        };
        if let Err(error) = gio(&["mount", uri.as_str()]) {
            // A phone that is locked or not set to "File transfer" refuses the mount.
            tracing::debug!(device = %device.name, %error, "gio does not mount the device");
            return Err(VfsError::DeviceUnavailable {
                path: device.root.path.clone(),
            });
        }
        // Mounted now: the next list names it without a mount.
        *self.volumes.lock_or_recover("gvfs.volumes") = None;
        Ok(None)
    }
}

#[cfg(test)]
#[path = "gvfs_tests.rs"]
mod tests;
