use std::io;

use super::{DeviceError, DeviceHub, NoDevices, PortableDevice};
use crate::VfsPath;

#[test]
fn a_device_failure_is_typed_on_its_path() {
    let cases = [
        (DeviceError::Unavailable, "device_unavailable"),
        (DeviceError::NotFound, "not_found"),
        (DeviceError::Exists, "already_exists"),
        (DeviceError::Denied, "permission_denied"),
        (DeviceError::Cancelled, "cancelled"),
        (DeviceError::Unsupported("x"), "unsupported"),
        (DeviceError::Io(io::Error::other("x")), "io"),
    ];
    for (failure, code) in cases {
        let error = failure.on("/Phone/DCIM");
        assert_eq!(error.code(), code);
        assert!(error.to_string().contains("/Phone/DCIM"), "{error}");
    }
}

#[test]
fn a_platform_without_devices_lists_none_and_opens_none() {
    assert!(NoDevices.devices().is_empty());
    let device = PortableDevice {
        id: "x".to_owned(),
        name: "Phone".to_owned(),
        root: VfsPath::local("/"),
        mount: None,
    };
    let opened = NoDevices.open(&device).map(|_| ());
    assert_eq!(
        opened.map_err(|error| error.code()),
        Err("device_unavailable")
    );
}
