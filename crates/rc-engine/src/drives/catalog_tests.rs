use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::broadcast;

use super::DriveCatalog;
use crate::{
    EngineEvent,
    drives::{Drive, DriveKind, DriveSpace, DriveState, fake::FakeDrives},
};

const LIMIT: Duration = Duration::from_millis(100);

fn catalog(
    fake: FakeDrives,
) -> (
    DriveCatalog,
    Arc<FakeDrives>,
    broadcast::Receiver<EngineEvent>,
) {
    let fake = Arc::new(fake);
    let (events, receiver) = broadcast::channel(16);
    let catalog = DriveCatalog::new(fake.clone(), LIMIT, events);
    (catalog, fake, receiver)
}

fn state(drives: &[Drive], id: &str) -> DriveState {
    drives
        .iter()
        .find(|drive| drive.id == id)
        .map(|drive| drive.state)
        .unwrap_or_else(|| panic!("no drive {id} in {drives:?}"))
}

#[tokio::test]
async fn a_local_drive_tells_its_volume_and_space_a_network_drive_only_when_refreshed() {
    let (catalog, fake, _events) = catalog(
        FakeDrives::default()
            .with("C:", DriveKind::Fixed, "/c")
            .with("N:", DriveKind::Network, "/n"),
    );
    let drives = catalog.list(false).await;
    assert_eq!(drives[0].state, DriveState::Ready);
    assert_eq!(drives[0].volume.as_deref(), Some("Volume C:"));
    assert_eq!(
        drives[0].space,
        Some(DriveSpace {
            free: 1_000,
            total: 4_000
        })
    );
    assert_eq!(state(&drives, "N:"), DriveState::Unknown);
    assert_eq!(fake.asked("N:"), 0, "a refresh never wakes a network share");

    let drives = catalog.list(true).await;
    assert_eq!(state(&drives, "N:"), DriveState::Ready);
    assert_eq!(fake.asked("N:"), 1);
}

#[tokio::test]
async fn a_fresh_answer_is_kept_and_not_asked_again() {
    let (catalog, fake, _events) =
        catalog(FakeDrives::default().with("C:", DriveKind::Fixed, "/c"));
    catalog.list(false).await;
    catalog.list(false).await;
    assert_eq!(fake.asked("C:"), 1);
}

#[tokio::test]
async fn a_drive_without_a_medium_is_not_ready() {
    let (catalog, _fake, _events) = catalog(
        FakeDrives::default()
            .with("D:", DriveKind::Optical, "/d")
            .failing("D:"),
    );
    let drives = catalog.list(false).await;
    assert_eq!(state(&drives, "D:"), DriveState::NotReady);
    assert_eq!(drives[0].space, None);
}

/// The acceptance's simulated time limit: a drive that hangs does not hold the list up, is not
/// asked again while it hangs, and its late answer is announced.
#[tokio::test]
async fn a_hanging_drive_does_not_hold_the_list_and_its_late_answer_is_announced() {
    let (catalog, fake, mut events) = catalog(
        FakeDrives::default()
            .with("C:", DriveKind::Fixed, "/c")
            .with("E:", DriveKind::Removable, "/e")
            .slow("E:", Duration::from_millis(600)),
    );
    let started = Instant::now();
    let drives = catalog.list(false).await;
    assert!(
        started.elapsed() < Duration::from_millis(450),
        "the list waited {:?}",
        started.elapsed()
    );
    assert_eq!(state(&drives, "C:"), DriveState::Ready);
    assert_eq!(state(&drives, "E:"), DriveState::Waiting);

    let again = catalog.list(true).await;
    assert_eq!(state(&again, "E:"), DriveState::Waiting);
    assert_eq!(catalog.probe("E:").await, DriveState::Waiting);
    assert_eq!(
        fake.asked("E:"),
        1,
        "a drive with an open question is not asked again"
    );

    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("the late answer is announced")
        .expect("an event");
    assert_eq!(event, EngineEvent::Drives);
    assert_eq!(state(&catalog.list(false).await, "E:"), DriveState::Ready);
}

#[tokio::test]
async fn probe_asks_one_drive_now() {
    let (catalog, fake, _events) = catalog(
        FakeDrives::default()
            .with("C:", DriveKind::Fixed, "/c")
            .with("N:", DriveKind::Network, "/n"),
    );
    assert_eq!(
        catalog.probe("N:").await,
        DriveState::Unknown,
        "not listed yet"
    );
    catalog.list(false).await;
    assert_eq!(catalog.probe("N:").await, DriveState::Ready);
    assert_eq!(fake.asked("N:"), 1);
    assert_eq!(fake.asked("C:"), 1);
}

#[tokio::test]
async fn an_unplugged_drive_leaves_the_list() {
    let (catalog, fake, _events) = catalog(
        FakeDrives::default()
            .with("C:", DriveKind::Fixed, "/c")
            .with("F:", DriveKind::Removable, "/f"),
    );
    assert_eq!(catalog.list(false).await.len(), 2);
    fake.unplug("F:");
    let drives = catalog.list(false).await;
    assert_eq!(drives.len(), 1);
    assert_eq!(drives[0].id, "C:");
}

/// RC-080-47: a drive list that hangs is asked once; the lists meanwhile show the last one, and
/// the late list is stored and announced.
#[tokio::test]
async fn a_hanging_drive_list_is_asked_once_and_its_late_answer_is_announced() {
    let (catalog, fake, mut events) = catalog(
        FakeDrives::default()
            .with("C:", DriveKind::Fixed, "/c")
            .slow_list(Duration::from_millis(400)),
    );
    let started = Instant::now();
    assert!(catalog.list(false).await.is_empty(), "nothing listed yet");
    assert!(catalog.list(true).await.is_empty());
    assert!(
        started.elapsed() < Duration::from_millis(350),
        "the lists waited {:?}",
        started.elapsed()
    );
    assert_eq!(fake.listed(), 1, "an open list question is not asked again");

    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("the late list is announced")
        .expect("an event");
    assert_eq!(event, EngineEvent::Drives);
    let late = catalog.list(false).await;
    assert_eq!(
        fake.listed(),
        2,
        "the question is closed: the next list asks"
    );
    assert_eq!(state(&late, "C:"), DriveState::Ready);
}
