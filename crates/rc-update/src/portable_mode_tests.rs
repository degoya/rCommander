use super::*;

fn installation(kind: InstallKind, folder: &Path) -> Installation {
    Installation {
        kind,
        reason: None,
        install_dir: folder.to_path_buf(),
        executable: "rcommander.exe".to_owned(),
    }
}

#[test]
fn a_portable_package_gets_the_data_folder_beside_it() {
    let root = tempfile::tempdir().expect("tempdir");
    let found = check_with(
        &installation(InstallKind::Portable, root.path()),
        false,
        false,
    );
    assert_eq!(found, Ok(root.path().join(DATA_DIR)));
    // The copy of `run-windows.sh`: no `VERSION.txt`, but a folder it may write.
    let found = check_with(
        &installation(InstallKind::Unknown, root.path()),
        false,
        false,
    );
    assert_eq!(found, Ok(root.path().join(DATA_DIR)));
}

#[test]
fn every_refusal_has_its_code() {
    let root = tempfile::tempdir().expect("tempdir");
    for kind in [
        InstallKind::Scoop,
        InstallKind::Winget,
        InstallKind::Homebrew,
        InstallKind::Deb,
        InstallKind::Rpm,
    ] {
        assert_eq!(
            check_with(&installation(kind, root.path()), false, false),
            Err("update.portable_package_manager"),
            "{kind:?}"
        );
    }
    let portable = installation(InstallKind::Portable, root.path());
    assert_eq!(
        check_with(&portable, true, false),
        Err("update.portable_macos")
    );
    assert_eq!(
        check_with(&portable, false, true),
        Err("update.portable_network_drive")
    );
    let missing = installation(InstallKind::Unknown, &root.path().join("missing"));
    assert_eq!(
        check_with(&missing, false, false),
        Err("update.portable_not_writable")
    );
    for code in [
        "update.portable_macos",
        "update.portable_package_manager",
        "update.portable_network_drive",
        "update.portable_not_writable",
        "update.portable_linked_data",
    ] {
        assert!(REFUSAL_CODES.contains(&code), "{code}");
    }
}

#[cfg(unix)]
#[test]
fn a_linked_data_folder_has_no_portable_copy() {
    let root = tempfile::tempdir().expect("tempdir");
    let elsewhere = root.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).expect("folder");
    let program = root.path().join("program");
    std::fs::create_dir(&program).expect("folder");
    std::os::unix::fs::symlink(&elsewhere, program.join(DATA_DIR)).expect("link");
    assert_eq!(
        check_with(&installation(InstallKind::Portable, &program), false, false),
        Err("update.portable_linked_data")
    );
}

#[test]
fn the_marker_is_a_plain_file_beside_the_program() {
    let root = tempfile::tempdir().expect("tempdir");
    assert!(!marker_present(root.path()));
    std::fs::create_dir(root.path().join(MARKER)).expect("a folder of its name");
    assert!(!marker_present(root.path()), "a folder is no marker");
    std::fs::remove_dir(root.path().join(MARKER)).expect("remove");
    std::fs::write(root.path().join(MARKER), MARKER_TEXT).expect("marker");
    assert!(marker_present(root.path()));
}

#[test]
fn a_unc_path_is_a_network_drive() {
    // Only Windows reads `\\server\share` as a prefix; elsewhere it is a plain name.
    let unc = Path::new(r"\\nas\tools\rcommander");
    assert_eq!(on_network_drive(unc), cfg!(windows));
    assert!(!on_network_drive(Path::new("relative")));
}
