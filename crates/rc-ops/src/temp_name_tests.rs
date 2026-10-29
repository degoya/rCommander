use std::path::{Path, PathBuf};

use super::{case_path, is_temp_name, pack_path, part_path};

#[test]
fn a_part_file_sits_beside_its_target() {
    assert_eq!(
        part_path(Path::new("/up/big.bin")),
        PathBuf::from("/up/big.bin.rcpart")
    );
}

#[test]
fn the_temporary_archive_is_hidden_beside_the_target() {
    assert_eq!(
        pack_path(Path::new("/out/site.zip"), 7),
        PathBuf::from("/out/.site.zip.7.rcpart")
    );
}

#[test]
fn a_case_rename_parks_under_a_hidden_name_in_the_same_directory() {
    let temp = case_path(Path::new("/d"), 0xab, 3);
    assert_eq!(temp.parent(), Some(Path::new("/d")));
    let name = temp
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    assert!(
        name.starts_with(".rc-tmp-") && name.ends_with("-ab-3"),
        "{name}"
    );
}

#[test]
fn every_scheme_is_known_as_a_temporary_name() {
    let case = case_path(Path::new("/d"), 1, 2);
    let case = case
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    for name in [
        "big.bin.rcpart",
        ".site.zip.7.rcpart",
        case.as_deref().unwrap_or_default(),
        ".rc-rename-t-1",
        ".rc-rename-4",
    ] {
        assert!(is_temp_name(name), "{name:?}");
    }
    for name in ["big.bin", "notes.rcpart.txt", ".rc-temp", "rc-tmp-1"] {
        assert!(!is_temp_name(name), "{name:?}");
    }
}
