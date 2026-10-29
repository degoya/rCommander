use std::fs;

use super::*;

/// The names in `dir` but `keep`.
fn others(dir: &Path, keep: &[&str]) -> Vec<String> {
    fs::read_dir(dir)
        .expect("listed")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| !keep.contains(&name.as_str()))
        .collect()
}

#[test]
fn a_file_is_written_and_no_temporary_name_stays() {
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("all.rcexport");
    write_private_file(&target, b"content", false).expect("written");
    assert_eq!(fs::read(&target).expect("read"), b"content");
    assert!(others(dir.path(), &["all.rcexport"]).is_empty());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&target).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn an_existing_target_stays_unless_overwritten() {
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("all.rcexport");
    fs::write(&target, "keep").expect("there");
    let refused = write_private_file(&target, b"new", false);
    assert!(matches!(refused, Err(BackupError::TargetExists { .. })));
    assert_eq!(fs::read_to_string(&target).expect("read"), "keep");
    assert!(others(dir.path(), &["all.rcexport"]).is_empty());
    write_private_file(&target, b"new", true).expect("replaced");
    assert_eq!(fs::read_to_string(&target).expect("read"), "new");
}

/// A link planted at the target, or at the old fixed staging name, is never written through.
#[cfg(unix)]
#[test]
fn a_planted_link_is_not_followed() {
    let dir = tempfile::tempdir().expect("dir");
    let victim = dir.path().join("victim");
    fs::write(&victim, "precious").expect("victim");
    let target = dir.path().join("all.rcexport");
    std::os::unix::fs::symlink(&victim, dir.path().join("all.rcexport.part")).expect("link");
    std::os::unix::fs::symlink(&victim, &target).expect("link");
    let refused = write_private_file(&target, b"new", false);
    assert!(matches!(refused, Err(BackupError::TargetExists { .. })));
    write_private_file(&target, b"new", true).expect("replaced");
    assert_eq!(fs::read_to_string(&victim).expect("read"), "precious");
    assert!(
        !fs::symlink_metadata(&target)
            .expect("meta")
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(&target).expect("read"), "new");
}
