use std::{path::Path, sync::Arc, time::Duration};

use rc_vfs::VfsPath;

use crate::test_support::expect_drives;
use crate::{
    Command, CommandClass, DriveKind, Engine, EngineError, EngineEvent, PanelId,
    drives::fake::FakeDrives,
};

async fn engine(fake: FakeDrives, start: &Path) -> Engine {
    Engine::open_with(
        VfsPath::local(start),
        VfsPath::local(start),
        Arc::new(fake),
        Duration::from_millis(100),
    )
    .await
    .expect("engine")
}

/// `C:` and the network drive `Y:` (`\\nas\media`, files open on it), all at `root`.
async fn setup() -> (tempfile::TempDir, Engine) {
    let root = tempfile::tempdir().expect("temp dir");
    let fake = FakeDrives::default()
        .with("C:", DriveKind::Fixed, root.path())
        .network("Y:", root.path().join("y"), r"\\nas\media")
        .busy("Y:");
    let engine = engine(fake, root.path()).await;
    (root, engine)
}

fn map(remote: &str, letter: &str) -> Command {
    Command::MapNetworkDrive {
        remote: remote.to_owned(),
        letter: letter.to_owned(),
        reconnect: true,
    }
}

fn unmap(drive: &str, force: bool) -> Command {
    Command::UnmapNetworkDrive {
        drive: drive.to_owned(),
        force,
    }
}

async fn code(engine: &Engine, command: Command) -> &'static str {
    engine.dispatch(command).await.expect_err("refused").code()
}

#[tokio::test]
async fn a_mapped_drive_appears_with_its_folder_and_is_announced() {
    let (_root, engine) = setup().await;
    let mut events = engine.subscribe();
    let drives = expect_drives(
        engine
            .dispatch(map("//nas/backup/", "z"))
            .await
            .expect("mapped"),
    );
    let mapped = drives.iter().find(|drive| drive.id == "Z:").expect("Z:");
    assert_eq!(mapped.kind, DriveKind::Network);
    assert_eq!(mapped.remote.as_deref(), Some(r"\\nas\backup"));
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::Drives));
    // The listed network drive shows its folder too, and only it has one.
    let listed = drives.iter().find(|drive| drive.id == "Y:").expect("Y:");
    assert_eq!(listed.remote.as_deref(), Some(r"\\nas\media"));
    let fixed = drives.iter().find(|drive| drive.id == "C:").expect("C:");
    assert_eq!(fixed.remote, None);
}

/// Acceptance: the input is checked before Windows sees it.
#[tokio::test]
async fn no_unc_folder_a_taken_letter_and_bad_characters_are_refused() {
    let (_root, engine) = setup().await;
    assert_eq!(
        code(&engine, map(r"C:\data", "Z")).await,
        "network_path_invalid"
    );
    assert_eq!(
        code(&engine, map(r"\\nas\me*dia", "Z")).await,
        "network_path_invalid"
    );
    assert_eq!(
        code(&engine, map(r"\\nas\media", "Y:")).await,
        "drive_letter_in_use"
    );
    assert_eq!(
        code(&engine, map(r"\\nas\media", "c")).await,
        "drive_letter_in_use"
    );
    assert_eq!(
        code(&engine, map(r"\\nas\media", "1")).await,
        "drive_letter_invalid"
    );
    assert_eq!(
        code(&engine, map(r"\\nas\media", "")).await,
        "drive_letter_invalid"
    );
}

#[tokio::test]
async fn windows_refusals_keep_their_meaning() {
    let (_root, engine) = setup().await;
    assert_eq!(
        code(&engine, map(r"\\offline\share", "Z")).await,
        "network_path_not_found"
    );
    let cancelled = engine
        .dispatch(map(r"\\nas\cancel", "Z"))
        .await
        .expect_err("cancelled");
    assert!(matches!(cancelled, EngineError::Cancelled), "{cancelled:?}");
}

/// Acceptance: files open on the drive - the first call is refused, a forced one disconnects.
#[tokio::test]
async fn a_busy_drive_is_disconnected_only_with_force() {
    let (_root, engine) = setup().await;
    assert_eq!(
        code(&engine, unmap("Y", false)).await,
        "network_drive_in_use"
    );
    let drives = expect_drives(engine.dispatch(unmap("y:", true)).await.expect("forced"));
    assert!(drives.iter().all(|drive| drive.id != "Y:"), "{drives:?}");
    assert_eq!(code(&engine, unmap("Y:", true)).await, "drive_not_found");
}

#[tokio::test]
async fn a_drive_that_is_no_network_drive_is_not_disconnected() {
    let (_root, engine) = setup().await;
    assert_eq!(code(&engine, unmap("C:", true)).await, "drive_not_network");
    assert_eq!(code(&engine, unmap("Q:", false)).await, "drive_not_found");
}

/// Acceptance: MCP cannot map (the credential dialog needs a person); disconnecting is
/// destructive, which MCP's policy confirms (`rc-mcp`'s `drives_tests`).
#[tokio::test]
async fn mcp_cannot_map_and_disconnecting_is_destructive() {
    let (_root, engine) = setup().await;
    let refused = engine
        .dispatch_mcp(map(r"\\nas\backup", "Z"))
        .await
        .expect_err("gui only");
    assert_eq!(refused.code(), "gui_only");
    assert_eq!(map("x", "Z").spec().class, CommandClass::GuiOnly);
    assert_eq!(unmap("Y", false).spec().class, CommandClass::Destructive);
}

#[cfg(not(windows))]
#[tokio::test]
async fn the_system_outside_windows_maps_nothing() {
    let (_root, engine) = crate::test_support::engine(&[]).await;
    assert_eq!(
        code(&engine, map(r"\\nas\media", "Z")).await,
        "network_drives_unsupported"
    );
}

fn change(drive: &str) -> Command {
    Command::ChangeDrive {
        panel: Some(PanelId::Left),
        drive: drive.to_owned(),
    }
}

/// A drive Windows remembers but has not connected since sign-in does not answer until it is
/// connected; changing to it connects it, as Explorer and Total Commander do.
#[tokio::test]
async fn a_remembered_drive_not_connected_is_connected_when_a_panel_changes_to_it() {
    let root = tempfile::tempdir().expect("temp dir");
    let share = root.path().join("z");
    std::fs::create_dir(&share).expect("mkdir");
    let fake = FakeDrives::default()
        .with("C:", DriveKind::Fixed, root.path())
        .network("Z:", &share, r"\\nas\backup")
        .disconnected("Z:");
    let engine = engine(fake, root.path()).await;
    engine
        .dispatch(change("Z:"))
        .await
        .expect("connects and changes");
    assert_eq!(engine.panel(PanelId::Left).path.path, share);
}

#[tokio::test]
async fn a_remembered_drive_whose_server_is_gone_stays_not_ready() {
    let root = tempfile::tempdir().expect("temp dir");
    let fake = FakeDrives::default()
        .with("C:", DriveKind::Fixed, root.path())
        .network("Z:", root.path().join("z"), r"\\offline\backup")
        .disconnected("Z:");
    let engine = engine(fake, root.path()).await;
    assert_eq!(code(&engine, change("Z:")).await, "drive_not_ready");
    assert_eq!(engine.panel(PanelId::Left).path.path, root.path());
}
