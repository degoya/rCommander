use std::fs;

use rc_vfs::BackendId;

use super::{Session, create, folders, owner, read_session, remove, session_path, write_session};
use crate::{
    VfsPath,
    remote_edit::{Baseline, Stamp},
};

fn session() -> Session {
    let remote = VfsPath {
        backend: BackendId::Ftp { connection_id: 7 },
        path: "/etc/app.conf".into(),
    };
    Session::new(
        (remote.clone(), remote),
        "web".into(),
        "app.conf".into(),
        Baseline {
            stamp: Stamp {
                size: 3,
                modified: None,
            },
            mode: Some(0o600),
        },
        None,
        false,
    )
}

#[test]
fn only_edit_pid_random_names_have_an_owner() {
    assert_eq!(owner("edit-1234-aB3"), Some(1234));
    for name in [
        "edit-1234-aB3.json",
        "edit-1234",
        "edit--7",
        "open-1234-7",
        "edit-x-7",
        "Edit-12-7",
    ] {
        assert_eq!(owner(name), None, "{name}");
    }
}

#[test]
fn a_folder_is_the_users_own_and_named_after_the_process() {
    let root = tempfile::tempdir().expect("temp dir");
    let dir = create(root.path()).expect("folder");
    let name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .expect("name");
    assert_eq!(owner(name), Some(std::process::id()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&dir).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    assert_eq!(folders(root.path(), |pid| pid == std::process::id()), [dir]);
}

#[test]
fn a_session_is_read_back_and_removed_with_its_folder() {
    let root = tempfile::tempdir().expect("temp dir");
    let dir = create(root.path()).expect("folder");
    fs::write(dir.join("app.conf"), b"abc").expect("copy");
    assert_eq!(read_session(&dir), None);
    write_session(&dir, &session()).expect("written");
    assert_eq!(read_session(&dir), Some(session()));

    remove(&dir);
    assert!(!dir.exists());
    assert!(!session_path(&dir).exists());
}

#[test]
fn a_session_of_another_version_or_shape_is_not_read() {
    let root = tempfile::tempdir().expect("temp dir");
    let dir = create(root.path()).expect("folder");
    fs::write(session_path(&dir), b"{\"version\": 2}").expect("write");
    assert_eq!(read_session(&dir), None);
    let mut other = serde_json::to_value(session()).expect("json");
    other["version"] = 9.into();
    fs::write(session_path(&dir), other.to_string()).expect("write");
    assert_eq!(read_session(&dir), None);
}
