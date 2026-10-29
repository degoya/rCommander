use std::path::Path;

use super::{SystemTrash, Trash, on_network};

#[test]
fn local_paths_are_not_on_a_network() {
    assert!(!on_network(Path::new("/home/user/file")));
    if cfg!(windows) {
        assert!(!on_network(Path::new(r"\\?\C:\Users\file")));
        assert!(!on_network(Path::new(r"C:\Users\file")));
    }
}

#[cfg(windows)]
#[test]
fn shares_and_wsl_are_on_a_network() {
    assert!(on_network(Path::new(r"\\?\UNC\nas\share\file")));
    assert!(on_network(Path::new(r"\\nas\share\file")));
    assert!(on_network(Path::new(r"\\?\UNC\wsl.localhost\Ubuntu\home")));
}

#[cfg(not(any(windows, target_os = "macos")))]
#[tokio::test]
async fn without_a_platform_bin_trash_is_unavailable() {
    assert!(SystemTrash::for_this_platform().is_none());
    let refused = SystemTrash
        .trash(Path::new("/tmp/x"))
        .await
        .expect_err("no bin");
    assert_eq!(refused.code(), "trash_unavailable");
}

#[cfg(windows)]
#[tokio::test]
async fn a_file_goes_into_the_recycle_bin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("rc-ops-trash-test.txt");
    std::fs::write(&file, b"bin").expect("write");
    SystemTrash.trash(&file).await.expect("trashed");
    assert!(!file.exists());
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_file_goes_into_the_trash() {
    assert!(SystemTrash::for_this_platform().is_some());
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("rc-ops-trash-test.txt");
    std::fs::write(&file, b"bin").expect("write");
    SystemTrash.trash(&file).await.expect("trashed");
    assert!(!file.exists());
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_link_goes_into_the_trash_and_its_target_stays() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("kept");
    std::fs::create_dir(&target).expect("target");
    let link = dir.path().join("rc-ops-trash-link");
    std::os::unix::fs::symlink(&target, &link).expect("link");
    SystemTrash.trash(&link).await.expect("trashed");
    assert!(std::fs::symlink_metadata(&link).is_err());
    assert!(target.is_dir());
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_missing_entry_is_a_file_system_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let refused = SystemTrash
        .trash(&dir.path().join("gone"))
        .await
        .expect_err("missing");
    assert_ne!(refused.code(), "trash_failed");
}
