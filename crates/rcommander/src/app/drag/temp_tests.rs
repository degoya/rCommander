use std::fs;

use super::*;

/// Sets the modification time of `path` (a folder) to `age` ago.
fn age(path: &Path, age: Duration) {
    let then = SystemTime::now() - age;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        // FILE_WRITE_ATTRIBUTES sets the time; FILE_FLAG_BACKUP_SEMANTICS opens a folder.
        options.access_mode(0x100).custom_flags(0x0200_0000);
    }
    options
        .open(path)
        .expect("open")
        .set_modified(then)
        .expect("touch");
}

#[test]
fn a_session_gets_its_own_folder_and_numbered_fetches() {
    let root = tempfile::tempdir().expect("temp dir");
    let folder = make_session(root.path()).expect("session");
    assert!(folder.is_dir());
    assert!(
        folder
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(PREFIX))
    );
    let mut area = TempArea::default();
    assert_eq!(area.next_folder(), None, "no session yet");
    area.set_session(folder.clone());
    assert_eq!(area.next_folder(), Some(folder.join("1")));
    assert_eq!(area.next_folder(), Some(folder.join("2")));
    let second = make_session(root.path()).expect("another session");
    assert_ne!(second, folder);
}

#[cfg(unix)]
#[test]
fn the_session_folder_is_only_this_users() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().expect("temp dir");
    let folder = make_session(root.path()).expect("session");
    let mode = fs::metadata(&folder).expect("meta").permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn closing_removes_everything_fetched() {
    let root = tempfile::tempdir().expect("temp dir");
    let folder = make_session(root.path()).expect("session");
    let mut area = TempArea::default();
    area.set_session(folder.clone());
    let fetch = area.next_folder().expect("a fetch folder");
    fs::create_dir_all(fetch.join("sub")).expect("mkdir");
    fs::write(fetch.join("sub/a.txt"), b"a").expect("write");
    area.clean_up().expect("cleaned");
    assert!(!folder.exists());
    assert_eq!(area.session(), None);
    area.clean_up().expect("nothing left to clean");
}

#[test]
fn dropping_the_area_removes_its_folder() {
    let root = tempfile::tempdir().expect("temp dir");
    let folder = make_session(root.path()).expect("session");
    fs::write(folder.join("a.txt"), b"a").expect("write");
    let mut area = TempArea::default();
    area.set_session(folder.clone());
    drop(area);
    assert!(!folder.exists());
}

#[test]
fn only_stale_session_folders_are_swept() {
    let root = tempfile::tempdir().expect("temp dir");
    let stale = root.path().join(format!("{PREFIX}1-old"));
    let fresh = root.path().join(format!("{PREFIX}2-new"));
    let other = root.path().join("someone-else");
    for folder in [&stale, &fresh, &other] {
        fs::create_dir(folder).expect("mkdir");
        fs::write(folder.join("f"), b"f").expect("write");
    }
    age(&stale, STALE_AFTER * 2);
    age(&other, STALE_AFTER * 2);
    assert_eq!(sweep(root.path(), SystemTime::now()), 1);
    assert!(!stale.exists());
    assert!(fresh.exists());
    assert!(other.exists());
}

#[cfg(unix)]
#[test]
fn a_sweep_follows_no_link() {
    let root = tempfile::tempdir().expect("temp dir");
    let target = tempfile::tempdir().expect("elsewhere");
    fs::write(target.path().join("keep"), b"k").expect("write");
    age(target.path(), STALE_AFTER * 2);
    let link = root.path().join(format!("{PREFIX}3-link"));
    std::os::unix::fs::symlink(target.path(), &link).expect("link");
    assert_eq!(sweep(root.path(), SystemTime::now()), 0);
    assert!(target.path().join("keep").exists());
}
