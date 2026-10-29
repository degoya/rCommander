use std::path::{Path, PathBuf};

use super::{
    PackFormat, PackSettings,
    members::{is_itself, member_name},
    own_archive,
};
use crate::walk::Item;

#[test]
fn every_format_has_its_ending_and_serde_name() {
    let names: Vec<String> = PackFormat::ALL
        .iter()
        .map(|format| {
            serde_json::to_value(format)
                .map(|v| v.to_string())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        names,
        [
            "\"zip\"",
            "\"7z\"",
            "\"tar\"",
            "\"tar_gz\"",
            "\"tar_xz\"",
            "\"tar_bz2\"",
            "\"tar_zst\""
        ]
    );
    let endings: Vec<&str> = PackFormat::ALL.iter().map(|f| f.extension()).collect();
    assert_eq!(
        endings,
        ["zip", "7z", "tar", "tar.gz", "tar.xz", "tar.bz2", "tar.zst"]
    );
}

#[test]
fn the_defaults_are_total_commanders() {
    let settings = PackSettings::default();
    assert_eq!(settings.format, PackFormat::Zip);
    assert!(settings.recurse && settings.keep_paths && settings.solid);
    assert!(!settings.delete_sources && !settings.overwrite && !settings.each_own_archive);
}

#[test]
fn names_keep_their_folders_with_slashes_or_only_the_file_name() {
    let item = Item {
        path: PathBuf::from("/src/docs/a.txt"),
        rel: Path::new("docs").join("a.txt"),
        entry: rc_vfs::Entry {
            name: "a.txt".to_owned(),
            kind: rc_vfs::EntryKind::File,
            size: 1,
            modified: None,
            created: None,
            attributes: rc_vfs::Attributes::default(),
            hidden: false,
            owner: None,
        },
    };
    assert_eq!(member_name(&item, true), "docs/a.txt");
    assert_eq!(member_name(&item, false), "a.txt");
}

/// RC-080-43: a source's archive of its own is named only from a plain file name; a listed name
/// that would lead out of the target folder, or that Windows refuses, is refused.
#[test]
fn an_own_archive_is_named_only_from_a_plain_name() {
    let dir = Path::new("/out");
    let named = own_archive(dir, "docs", "zip").expect("a plain name");
    assert_eq!(named, PathBuf::from("/out/docs.zip"));
    for name in ["../evil", "a/b", "a\\b", "con", "a:b"] {
        let refused = own_archive(dir, name, "zip").map_err(|error| error.code());
        assert_eq!(refused, Err("invalid_name"), "{name}");
    }
}

#[test]
fn the_archive_itself_is_found_in_the_case_of_its_disk() {
    let dest = Path::new("/data/archive.zip");
    assert!(is_itself(Path::new("/data/archive.zip"), dest, true));
    assert!(!is_itself(Path::new("/data/Archive.zip"), dest, true));
    assert!(is_itself(Path::new("/data/Archive.zip"), dest, false));
    assert!(is_itself(Path::new("/DATA/ARCHIVE.ZIP"), dest, false));
    assert!(!is_itself(Path::new("/data/archive.zip.bak"), dest, false));
}
