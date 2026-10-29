use super::*;
use crate::install::fixture::write;

#[test]
fn the_unpacked_package_is_portable() {
    let root = tempfile::tempdir().expect("tempdir");
    let exe = root.path().join("rcommander.exe");
    write(&exe, "program");
    write(&root.path().join("VERSION.txt"), "rCommander 1.0.0");
    let found = detect(&exe);
    assert_eq!(found.kind, InstallKind::Portable);
    assert_eq!(found.reason, None);
    assert_eq!(found.install_dir, root.path());
    assert_eq!(found.executable, "rcommander.exe");
}

/// A build from source and the copy of `scripts/run-windows.sh` carry no `VERSION.txt`.
#[test]
fn a_program_without_its_version_file_is_unknown() {
    let root = tempfile::tempdir().expect("tempdir");
    let exe = root.path().join("rcommander.exe");
    write(&exe, "program");
    let found = detect(&exe);
    assert_eq!(found.kind, InstallKind::Unknown);
    assert_eq!(found.reason, Some("update.kind_no_version_file"));
}

#[test]
fn a_marker_of_another_kind_makes_it_unknown_and_portable_keeps_it() {
    let root = tempfile::tempdir().expect("tempdir");
    let exe = root.path().join("rcommander.exe");
    write(&exe, "program");
    write(&root.path().join("VERSION.txt"), "rCommander 1.0.0");
    write(
        &root.path().join(INSTALL_KIND_FILE),
        "# written by the installer\nmsi\n",
    );
    assert_eq!(detect(&exe).reason, Some("update.kind_marker"));
    write(&root.path().join(INSTALL_KIND_FILE), "portable\n");
    assert_eq!(detect(&exe).kind, InstallKind::Portable);
}

#[test]
fn a_macos_bundle_is_installed_as_its_folder_entry() {
    let root = tempfile::tempdir().expect("tempdir");
    let app = root.path().join("rCommander.app");
    let exe = app.join("Contents").join("MacOS").join("rcommander");
    write(&exe, "program");
    write(
        &app.join("Contents").join("Resources").join("VERSION.txt"),
        "rCommander 1.0.0",
    );
    let found = detect(&exe);
    assert_eq!(found.kind, InstallKind::Portable);
    assert_eq!(found.install_dir, root.path());
    assert_eq!(found.executable, "rCommander.app/Contents/MacOS/rcommander");
}

#[cfg(unix)]
#[test]
fn a_folder_this_process_cannot_write_is_unknown() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().expect("tempdir");
    let exe = root.path().join("rcommander");
    write(&exe, "program");
    write(&root.path().join("VERSION.txt"), "rCommander 1.0.0");
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o555)).expect("lock");
    let probe_writes = fs::write(root.path().join("probe"), "").is_ok();
    let found = detect(&exe);
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).expect("unlock");
    if probe_writes {
        // Permissions do not hold for this user (root); nothing to prove.
        return;
    }
    assert_eq!(found.reason, Some("update.kind_not_writable"));
}
