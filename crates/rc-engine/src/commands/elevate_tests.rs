use std::path::Path;

use super::*;
use crate::{DriveGroup, DriveState};

fn network(id: &str, remote: &str) -> Drive {
    Drive {
        id: id.into(),
        group: DriveGroup::Drives,
        kind: DriveKind::Network,
        label: id.into(),
        root: None,
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        folder_buttons: Vec::new(),
        favourite: false,
        remote: Some(remote.into()),
    }
}

/// The elevated process does not see the session's drive letters: a folder on a mapped drive
/// goes as its UNC path, any other as it is.
#[test]
fn a_mapped_drive_goes_as_its_unc_path() {
    let drives = [
        network("Z:", "\\\\server\\share"),
        network("Y:", "\\\\nas\\data\\"),
    ];
    assert_eq!(
        unc_folder("Z:\\work\\2026", &drives),
        "\\\\server\\share\\work\\2026"
    );
    assert_eq!(unc_folder("z:\\", &drives), "\\\\server\\share");
    assert_eq!(unc_folder("Y:\\a", &drives), "\\\\nas\\data\\a");
    assert_eq!(unc_folder("C:\\Windows", &drives), "C:\\Windows");
    let mut fixed = network("C:", "\\\\not\\used");
    fixed.kind = DriveKind::Fixed;
    assert_eq!(unc_folder("C:\\x", &[fixed]), "C:\\x");
}

/// `pushd` into the folder first, `/k` with Shift+Enter, and a leading `cmd` is the system
/// folder's, never one in the panel's folder.
#[test]
fn the_elevated_cmd_goes_to_the_folder_first() {
    let cmd = Path::new("C:\\Windows\\System32\\cmd.exe");
    assert_eq!(
        elevated_cmd_arguments("\\\\server\\share\\work", "dir /b", false, cmd),
        "/s /c \"pushd \"\\\\server\\share\\work\" && dir /b\""
    );
    assert_eq!(
        elevated_cmd_arguments("C:\\Work", "cmd", true, cmd),
        "/s /k \"pushd \"C:\\Work\" && \"C:\\Windows\\System32\\cmd.exe\"\""
    );
    assert_eq!(
        elevated_cmd_arguments("C:\\Work", "CMD.EXE /k ver", false, cmd),
        "/s /c \"pushd \"C:\\Work\" && \"C:\\Windows\\System32\\cmd.exe\" /k ver\""
    );
}

/// `*` outside Windows: refused, nothing starts.
#[cfg(not(windows))]
#[tokio::test]
async fn an_elevated_line_outside_windows_starts_nothing() {
    use crate::recover::LockOrRecover as _;
    let (root, engine) = crate::test_support::engine(&[]).await;
    let error = engine
        .run_elevated_line("cmd", root.path(), false)
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "run_as_admin_unsupported");
    assert!(STARTED.lock_or_recover("test").is_empty());
}

/// On Windows the line goes to the system folder's cmd as administrator.
#[cfg(windows)]
#[tokio::test]
async fn an_elevated_line_starts_the_system_cmd_as_administrator() {
    use crate::recover::LockOrRecover as _;
    let (root, engine) = crate::test_support::engine(&[]).await;
    engine
        .run_elevated_line("dir", root.path(), false)
        .await
        .expect("started");
    let started = STARTED.lock_or_recover("test").pop().expect("recorded");
    assert!(started.elevated);
    assert!(started.program.ends_with("cmd.exe"));
    assert!(
        started.params.starts_with("/s /c \"pushd \""),
        "{}",
        started.params
    );
    assert!(started.params.ends_with(" && dir\""), "{}", started.params);
}
