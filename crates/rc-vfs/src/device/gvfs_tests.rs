use std::path::{Path, PathBuf};

use super::{Volume, devices, mounted_hosts, parse_gio};
use crate::BackendId;

/// What `gio mount -li` writes for a phone gvfs knows but has not mounted, beside a USB disk and
/// a mounted second phone (abridged).
const GIO: &str = "\
Drive(0): SanDisk Ultra
  Type: GProxyDrive (GProxyVolumeMonitorUDisks2)
  Volume(0): STICK
    Type: GProxyVolume (GProxyVolumeMonitorUDisks2)
    activation_root=file:///media/stick/
Volume(0): Pixel 8
  Type: GProxyVolume (GProxyVolumeMonitorMTP)
  ids:
   unix-device: '/dev/bus/usb/001/005'
  activation_root=mtp://Google_Pixel_8_37281FDJH0012X/
  themed icons:  [phone]  [phone-symbolic]
  can_mount=1
Mount(0): Galaxy S23 -> mtp://SAMSUNG_SAMSUNG_Android_R5CW123/
  Type: GDaemonMount
  default_location=mtp://SAMSUNG_SAMSUNG_Android_R5CW123/
";

#[test]
fn gio_names_the_mtp_volumes_and_mounts_once_each() {
    assert_eq!(
        parse_gio(GIO),
        [
            Volume {
                name: "Pixel 8".to_owned(),
                host: "Google_Pixel_8_37281FDJH0012X".to_owned(),
            },
            Volume {
                name: "Galaxy S23".to_owned(),
                host: "SAMSUNG_SAMSUNG_Android_R5CW123".to_owned(),
            },
        ]
    );
    assert!(parse_gio("").is_empty());
}

#[test]
fn the_gvfs_folder_names_the_mounted_devices() {
    let names = [
        "mtp:host=SAMSUNG_SAMSUNG_Android_R5CW123",
        "smb-share:server=nas",
        "mtp:host=",
    ];
    assert_eq!(mounted_hosts(names), ["SAMSUNG_SAMSUNG_Android_R5CW123"]);
}

#[test]
fn a_device_is_its_gvfs_folder_on_the_local_backend() {
    let gvfs = Path::new("/run/user/1000/gvfs");
    let mounted = vec![
        "SAMSUNG_SAMSUNG_Android_R5CW123".to_owned(),
        "Old_Phone".to_owned(),
    ];
    let found = devices(gvfs, &mounted, &parse_gio(GIO));
    let summary: Vec<(&str, &str, Option<&str>)> = found
        .iter()
        .map(|device| {
            (
                device.id.as_str(),
                device.name.as_str(),
                device.mount.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("mtp:SAMSUNG_SAMSUNG_Android_R5CW123", "Galaxy S23", None),
            ("mtp:Old_Phone", "Old Phone", None),
            (
                "mtp:Google_Pixel_8_37281FDJH0012X",
                "Pixel 8",
                Some("mtp://Google_Pixel_8_37281FDJH0012X/")
            ),
        ]
    );
    assert_eq!(found[0].root.backend, BackendId::Local);
    assert_eq!(
        found[2].root.path,
        PathBuf::from("/run/user/1000/gvfs/mtp:host=Google_Pixel_8_37281FDJH0012X"),
        "where gvfs will mount it"
    );
}
