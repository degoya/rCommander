use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use rc_db::Db;
use rc_vfs::{BackendId, MemoryFs, PortableDevice, Vfs, VfsPath};
use tempfile::TempDir;

use crate::{
    Command, DriveGroup, DriveKind, Engine, EngineError, Outcome, PanelId,
    drives::fake::{FakeDrives, FakeHub},
    settings::PORTABLE_DEVICES_KEY,
    test_support::expect_drives,
};

const PHONE: &str = r"\\?\usb#vid_18d1&pid_4ee1#1";

fn wpd_phone() -> PortableDevice {
    PortableDevice {
        id: PHONE.to_owned(),
        name: "Pixel 8".to_owned(),
        root: VfsPath {
            backend: BackendId::Device {
                device: PHONE.to_owned(),
            },
            path: PathBuf::from("/"),
        },
        mount: None,
    }
}

/// A hub with `device`, whose WPD devices open as `backend`.
fn hub(device: PortableDevice, backend: Arc<dyn Vfs>) -> Arc<FakeHub> {
    Arc::new(FakeHub {
        devices: std::sync::Mutex::new(vec![device]),
        backend: Some(backend),
        opened: Default::default(),
    })
}

async fn engine(hub: &Arc<FakeHub>, start: &Path) -> Engine {
    let fake = FakeDrives::default().with_devices(Arc::clone(hub));
    Engine::open_with(
        VfsPath::local(start),
        VfsPath::local(start),
        Arc::new(fake),
        Duration::from_millis(500),
    )
    .await
    .expect("engine")
}

async fn drives(engine: &Engine) -> Vec<crate::Drive> {
    expect_drives(
        engine
            .dispatch(Command::ListDrives { refresh: false })
            .await
            .expect("drives"),
    )
}

async fn change(engine: &Engine, drive: &str) -> Result<Outcome, EngineError> {
    engine
        .dispatch(Command::ChangeDrive {
            panel: Some(PanelId::Left),
            drive: drive.into(),
        })
        .await
}

/// A phone with its internal storage, as a memory file system.
async fn phone_files() -> Arc<dyn Vfs> {
    let fs = MemoryFs::new();
    fs.mkdir(Path::new("/Internal storage/DCIM"), true)
        .await
        .expect("mkdir");
    Arc::new(fs)
}

#[tokio::test]
async fn a_phone_is_an_entry_of_the_drive_bar_with_its_name() {
    let start = TempDir::new().expect("tempdir");
    let hub = hub(wpd_phone(), phone_files().await);
    let engine = engine(&hub, start.path()).await;
    let listed = drives(&engine).await;
    let phone = listed
        .iter()
        .find(|drive| drive.group == DriveGroup::Devices)
        .expect("the phone is listed");
    assert_eq!(phone.kind, DriveKind::Device);
    assert_eq!(phone.label, "Pixel 8");
    assert_eq!(phone.id, format!("device:{PHONE}"));
}

#[tokio::test]
async fn with_the_setting_off_no_device_is_listed() {
    let start = TempDir::new().expect("tempdir");
    let hub = hub(wpd_phone(), phone_files().await);
    let engine = engine(&hub, start.path()).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    engine
        .dispatch(Command::SetSetting {
            key: PORTABLE_DEVICES_KEY.into(),
            value: serde_json::json!(false),
        })
        .await
        .expect("stored");
    let listed = drives(&engine).await;
    assert!(
        listed
            .iter()
            .all(|drive| drive.group != DriveGroup::Devices)
    );
}

#[tokio::test]
async fn changing_to_a_phone_opens_it_and_shows_its_storages() {
    let start = TempDir::new().expect("tempdir");
    let hub = hub(wpd_phone(), phone_files().await);
    let engine = engine(&hub, start.path()).await;
    drives(&engine).await;
    change(&engine, &format!("device:{PHONE}"))
        .await
        .expect("changes to the phone");
    let panel = engine.panel(PanelId::Left);
    assert_eq!(panel.path, wpd_phone().root);
    assert_eq!(hub.opened.load(Ordering::SeqCst), 1);
    let names: Vec<&str> = panel
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert!(names.contains(&"Internal storage"), "{names:?}");
}

#[tokio::test]
async fn a_phone_that_shares_no_storage_is_unavailable_and_the_panel_stays() {
    let start = TempDir::new().expect("tempdir");
    let locked: Arc<dyn Vfs> = Arc::new(MemoryFs::new());
    let hub = hub(wpd_phone(), locked);
    let engine = engine(&hub, start.path()).await;
    drives(&engine).await;
    let refused = change(&engine, &format!("device:{PHONE}")).await;
    assert_eq!(refused.expect_err("locked").code(), "device_unavailable");
    assert_eq!(
        engine.panel(PanelId::Left).path,
        VfsPath::local(start.path())
    );
}

#[tokio::test]
async fn a_gvfs_phone_is_its_folder_on_the_local_backend() {
    let start = TempDir::new().expect("tempdir");
    // gvfs names the folder `mtp:host=...`; Windows refuses `:` in a name, and the engine never
    // reads the folder's name, so the test's folder goes without it.
    let mount = start.path().join("mtp-host-Google_Pixel_8");
    std::fs::create_dir_all(mount.join("Internal shared storage")).expect("mkdir");
    let device = PortableDevice {
        id: "mtp:Google_Pixel_8".to_owned(),
        name: "Pixel 8".to_owned(),
        root: VfsPath::local(&mount),
        mount: Some("mtp://Google_Pixel_8/".to_owned()),
    };
    let hub = hub(device, phone_files().await);
    let engine = engine(&hub, start.path()).await;
    drives(&engine).await;
    change(&engine, "device:mtp:Google_Pixel_8")
        .await
        .expect("changes to the phone");
    assert_eq!(engine.panel(PanelId::Left).path, VfsPath::local(&mount));
    assert_eq!(hub.opened.load(Ordering::SeqCst), 1, "mounted first");
}

/// `device://<name>/<path>` as a panel on this machine reads it: the phone's folder, never a
/// folder of that name on the local drive (RC-110-16).
#[tokio::test]
async fn a_device_location_names_the_phone_from_a_local_panel() {
    let start = TempDir::new().expect("tempdir");
    let hub = hub(wpd_phone(), phone_files().await);
    let engine = engine(&hub, start.path()).await;
    drives(&engine).await;
    let local = VfsPath::local(start.path());
    let located = engine
        .locate(&local, "device://Pixel 8/Internal storage/DCIM")
        .await
        .expect("the phone's folder");
    assert_eq!(located.backend, wpd_phone().root.backend);
    let names: Vec<_> = located.path.components().collect();
    assert_eq!(
        names,
        Path::new("/Internal storage/DCIM")
            .components()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        hub.opened.load(Ordering::SeqCst),
        1,
        "opened for the location"
    );
    let root = engine
        .locate(&local, "device://Pixel 8")
        .await
        .expect("the phone's root");
    assert_eq!(root, wpd_phone().root);
    assert_eq!(
        hub.opened.load(Ordering::SeqCst),
        1,
        "its session is used again"
    );
}

#[tokio::test]
async fn a_device_location_without_such_a_device_is_refused() {
    let start = TempDir::new().expect("tempdir");
    let hub = hub(wpd_phone(), phone_files().await);
    let engine = engine(&hub, start.path()).await;
    drives(&engine).await;
    let refused = engine
        .locate(
            &VfsPath::local(start.path()),
            "device://Pixel 9/Internal storage",
        )
        .await;
    assert_eq!(
        refused.expect_err("no such phone").code(),
        "drive_not_found"
    );
}

/// Two phones of one name: the name reaches neither, each one's id reaches it.
#[tokio::test]
async fn a_shared_device_name_is_refused_and_the_id_reaches_the_phone() {
    let start = TempDir::new().expect("tempdir");
    let hub = hub(wpd_phone(), phone_files().await);
    let twin = r"\\?\usb#vid_18d1&pid_4ee1#2";
    let mut second = wpd_phone();
    second.id = twin.to_owned();
    second.root.backend = BackendId::Device {
        device: twin.to_owned(),
    };
    hub.devices.lock().expect("devices").push(second);
    let engine = engine(&hub, start.path()).await;
    drives(&engine).await;
    let local = VfsPath::local(start.path());
    let refused = engine
        .locate(&local, "device://Pixel 8/Internal storage")
        .await;
    assert_eq!(refused.expect_err("ambiguous").code(), "drive_not_found");
    let located = engine
        .locate(&local, &format!("device://{twin}/Internal storage"))
        .await
        .expect("by its id");
    assert_eq!(
        located.backend,
        BackendId::Device {
            device: twin.to_owned()
        }
    );
}
