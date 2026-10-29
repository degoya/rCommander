use std::fs;

use super::{alive, create_folder, owner, sweep};

#[test]
fn only_open_pid_n_names_have_an_owner() {
    assert_eq!(owner("open-1234-7"), Some(1234));
    assert_eq!(owner("open-1234-aB3xYz"), Some(1234));
    for name in [
        "open-1234",
        "open--7",
        "open-1234-",
        "open-x-7",
        "open-+12-7",
        "open-12-7-1",
        "open-12-+7",
        "open-12-a.b",
        "Open-12-7",
        "rcommander.db",
        "open-99999999999-1",
    ] {
        assert_eq!(owner(name), None, "{name}");
    }
}

#[test]
fn a_sweep_removes_only_the_folders_of_the_picked_owners() {
    let root = tempfile::tempdir().expect("temp dir");
    let path = root.path();
    for dir in [
        "open-111-1",
        "open-111-2",
        "open-222-1",
        "open-111-x_y",
        "other",
    ] {
        fs::create_dir(path.join(dir)).expect("folder");
        fs::write(path.join(dir).join("copy.txt"), b"x").expect("file");
    }
    // A read-only copy, as the unpacking leaves it.
    let copy = path.join("open-111-1").join("copy.txt");
    let mut permissions = fs::metadata(&copy).expect("metadata").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&copy, permissions).expect("read-only");
    // A file with a folder's name is never removed.
    fs::write(path.join("open-111-3"), b"not a folder").expect("file");

    assert_eq!(sweep(path, |pid| pid == 111), 2);

    assert!(!path.join("open-111-1").exists());
    assert!(!path.join("open-111-2").exists());
    for kept in ["open-222-1", "open-111-x_y", "other", "open-111-3"] {
        assert!(path.join(kept).exists(), "{kept}");
    }
}

#[test]
fn a_missing_root_sweeps_nothing() {
    let root = tempfile::tempdir().expect("temp dir");
    assert_eq!(sweep(&root.path().join("absent"), |_| true), 0);
}

#[cfg(unix)]
#[test]
fn a_link_with_a_folders_name_is_neither_followed_nor_removed() {
    let root = tempfile::tempdir().expect("temp dir");
    let outside = tempfile::tempdir().expect("temp dir");
    fs::write(outside.path().join("keep.txt"), b"keep").expect("file");
    let link = root.path().join("open-111-1");
    std::os::unix::fs::symlink(outside.path(), &link).expect("link");

    assert_eq!(sweep(root.path(), |_| true), 0);

    assert!(link.symlink_metadata().is_ok());
    assert!(outside.path().join("keep.txt").exists());
}

#[test]
fn this_process_is_alive() {
    assert!(alive(std::process::id()));
}

#[cfg(target_os = "linux")]
#[test]
fn a_pid_without_a_process_is_not_alive() {
    assert!(!alive(u32::MAX));
}

#[test]
fn a_new_folder_has_a_name_nobody_guesses_and_is_the_users_alone() {
    let first = create_folder().expect("a folder");
    let second = create_folder().expect("another");
    for dir in [&first, &second] {
        assert!(dir.is_dir());
        let name = dir.file_name().and_then(|name| name.to_str());
        assert_eq!(name.and_then(owner), Some(std::process::id()), "{name:?}");
    }
    assert_ne!(first, second);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let root = first.parent().expect("the root");
        for dir in [root, first.as_path()] {
            let mode = fs::metadata(dir).expect("metadata").permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "{}", dir.display());
        }
    }
    fs::remove_dir(&first).expect("removed");
    fs::remove_dir(&second).expect("removed");
}

#[cfg(unix)]
#[test]
fn the_root_is_made_the_users_alone_and_a_link_is_refused() {
    use std::os::unix::fs::PermissionsExt as _;

    use super::own_root;

    let base = tempfile::tempdir().expect("temp dir");
    let wide = base.path().join("wide");
    fs::create_dir(&wide).expect("folder");
    fs::set_permissions(&wide, fs::Permissions::from_mode(0o777)).expect("wide open");
    own_root(&wide).expect("tightened");
    let mode = fs::metadata(&wide).expect("metadata").permissions().mode();
    assert_eq!(mode & 0o777, 0o700);

    let elsewhere = tempfile::tempdir().expect("temp dir");
    let link = base.path().join("link");
    std::os::unix::fs::symlink(elsewhere.path(), &link).expect("link");
    assert!(
        own_root(&link).is_err(),
        "a link to another folder is no root"
    );
}
