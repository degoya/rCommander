use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use rc_vfs::{BackendId, DeviceHub, PortableDevice, Vfs, VfsPath};
use tokio::sync::broadcast;

use super::{DeviceList, drive_id, entry};
use crate::{DriveGroup, DriveKind, EngineEvent, drives::fake::FakeHub};

fn phone(id: &str, name: &str) -> PortableDevice {
    PortableDevice {
        id: id.to_owned(),
        name: name.to_owned(),
        root: VfsPath {
            backend: BackendId::Device {
                device: id.to_owned(),
            },
            path: "/".into(),
        },
        mount: None,
    }
}

/// A hub whose list takes `delay`.
struct SlowHub {
    delay: Duration,
    devices: Mutex<Vec<PortableDevice>>,
}

impl DeviceHub for SlowHub {
    fn devices(&self) -> Vec<PortableDevice> {
        thread::sleep(self.delay);
        self.devices.lock().expect("lock").clone()
    }

    fn open(&self, _device: &PortableDevice) -> rc_vfs::Result<Option<Arc<dyn Vfs>>> {
        Ok(None)
    }
}

#[tokio::test]
async fn the_list_names_the_devices_by_their_drive_ids() {
    let hub = Arc::new(FakeHub::default());
    hub.devices
        .lock()
        .expect("lock")
        .push(phone("usb#vid_18d1", "Pixel 8"));
    let (events, _) = broadcast::channel(4);
    let list = DeviceList::new(hub, Duration::from_secs(1), events);
    let devices = list.list().await;
    assert_eq!(devices.len(), 1);
    let id = drive_id("usb#vid_18d1");
    assert_eq!(id, "device:usb#vid_18d1");
    assert_eq!(
        list.find(&id).map(|device| device.name),
        Some("Pixel 8".to_owned())
    );
    assert!(list.find("device:other").is_none());
}

#[tokio::test]
async fn a_device_list_that_hangs_leaves_the_last_one_and_announces_the_late_one() {
    let hub = Arc::new(SlowHub {
        delay: Duration::from_millis(300),
        devices: Mutex::new(vec![phone("a", "Phone")]),
    });
    let (events, mut received) = broadcast::channel(4);
    let list = DeviceList::new(hub, Duration::from_millis(20), events);
    let started = Instant::now();
    assert!(list.list().await.is_empty(), "nothing known yet");
    assert!(started.elapsed() < Duration::from_millis(250), "not held");
    let announced = tokio::time::timeout(Duration::from_secs(2), received.recv()).await;
    assert!(
        matches!(announced, Ok(Ok(EngineEvent::Drives))),
        "{announced:?}"
    );
    assert_eq!(list.list().await.len(), 1, "the late list stands");
}

#[test]
fn a_device_is_an_entry_of_its_own_group_with_the_phone_kind() {
    let drive = entry(&phone("usb#1", "Pixel 8"));
    assert_eq!(drive.id, "device:usb#1");
    assert_eq!(drive.group, DriveGroup::Devices);
    assert_eq!(drive.kind, DriveKind::Device);
    assert_eq!(drive.label, "Pixel 8");
    assert!(drive.root.is_some());
}
