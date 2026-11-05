use std::path::{Path, PathBuf};

use super::{DEFAULT_SYSTEM_ROOT, system32};

#[test]
fn an_absolute_system_root_is_taken() {
    let root = std::env::temp_dir();
    assert_eq!(system32(Some(&root)), root.join("System32"));
}

#[test]
fn a_missing_empty_or_relative_system_root_falls_back_to_the_default() {
    let fallback = PathBuf::from(DEFAULT_SYSTEM_ROOT).join("System32");
    assert_eq!(system32(None), fallback);
    assert_eq!(system32(Some(Path::new(""))), fallback);
    assert_eq!(system32(Some(Path::new("Windows"))), fallback);
    assert_eq!(system32(Some(Path::new(r".\evil"))), fallback);
}

#[cfg(windows)]
#[test]
fn a_root_without_a_drive_is_not_absolute_on_windows() {
    let fallback = PathBuf::from(r"C:\Windows\System32");
    assert_eq!(system32(Some(Path::new(r"\Windows"))), fallback);
    assert_eq!(system32(Some(Path::new("C:Windows"))), fallback);
    assert_eq!(
        system32(Some(Path::new(r"D:\Win"))),
        PathBuf::from(r"D:\Win\System32")
    );
}
