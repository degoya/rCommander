use std::path::{Path, PathBuf};

use rc_db::DataLocation;
use rc_update::{
    InstallKind, Installation,
    portable_mode::{DATA_DIR, MARKER},
};

use super::decide;

/// A program folder with the program and, when asked, the marker and `VERSION.txt` beside it.
fn program(marker: bool) -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("tempdir");
    let executable = root.path().join("rcommander.exe");
    std::fs::write(&executable, b"program").expect("program");
    std::fs::write(root.path().join("VERSION.txt"), b"1.0.0").expect("version");
    if marker {
        std::fs::write(root.path().join(MARKER), b"").expect("marker");
    }
    (root, executable)
}

/// The refusal `code`; on macOS every marker is passed over first.
fn refused(code: &'static str) -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("update.portable_macos")
    } else {
        Some(code)
    }
}

fn installed(kind: InstallKind, folder: &Path) -> Installation {
    Installation {
        kind,
        reason: None,
        install_dir: folder.to_path_buf(),
        executable: "rcommander.exe".to_owned(),
    }
}

#[test]
fn a_marker_beside_a_portable_package_switches() {
    let (root, executable) = program(true);
    let decision = decide(&executable, |_| {
        installed(InstallKind::Portable, root.path())
    });
    if cfg!(target_os = "macos") {
        assert_eq!(decision.portable, None);
        assert_eq!(decision.refusal, refused("update.portable_macos"));
    } else {
        assert_eq!(
            decision.portable,
            Some(DataLocation::Portable(root.path().join(DATA_DIR)))
        );
        assert_eq!(decision.refusal, None);
    }

    // Without the marker nothing changes, and nothing is asked of the installation.
    let (_root, executable) = program(false);
    let decision = decide(&executable, |_| panic!("no marker, no detection"));
    assert_eq!((decision.portable, decision.refusal), (None, None));
}

#[test]
fn a_marker_in_a_managers_folder_is_ignored() {
    let (root, executable) = program(true);
    for kind in [InstallKind::Winget, InstallKind::Scoop, InstallKind::Deb] {
        let decision = decide(&executable, |_| installed(kind, root.path()));
        assert_eq!(decision.portable, None, "{kind:?}");
        assert_eq!(decision.refusal, refused("update.portable_package_manager"));
    }
}

#[cfg(unix)]
#[test]
fn a_linked_data_folder_is_refused() {
    let (root, executable) = program(true);
    let elsewhere = tempfile::tempdir().expect("tempdir");
    std::os::unix::fs::symlink(elsewhere.path(), root.path().join(DATA_DIR)).expect("link");
    let decision = decide(&executable, |_| {
        installed(InstallKind::Portable, root.path())
    });
    assert_eq!(decision.portable, None);
    assert_eq!(decision.refusal, refused("update.portable_linked_data"));
}

#[test]
fn an_unwritable_folder_falls_back_with_a_notice() {
    let (root, executable) = program(true);
    // `install_kind` found no folder it may write (`C:\Program Files`).
    let gone = root.path().join("gone");
    let decision = decide(&executable, |_| installed(InstallKind::Unknown, &gone));
    assert_eq!(decision.portable, None);
    assert_eq!(decision.refusal, refused("update.portable_not_writable"));
    assert_ne!(
        crate::i18n::error_text("update.portable_not_writable"),
        "update.portable_not_writable",
        "the status line has a text for it"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let folder = root.path();
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o555)).expect("lock");
        let held = std::fs::write(folder.join("probe"), b"").is_err();
        if held {
            let decision = decide(&executable, |_| installed(InstallKind::Unknown, folder));
            assert_eq!(decision.refusal, refused("update.portable_not_writable"));
        }
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o755)).expect("unlock");
    }
}
