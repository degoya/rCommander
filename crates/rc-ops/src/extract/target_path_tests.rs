use std::path::{Path, PathBuf};

use super::{below, parts};

#[test]
fn plain_names_land_below_the_target() {
    assert_eq!(
        below(Path::new("/t"), &["docs", "Gr\u{f6}\u{df}e.txt"]),
        Ok(PathBuf::from("/t/docs/Gr\u{f6}\u{df}e.txt"))
    );
}

/// Acceptance (RC-030-03): nothing an archive stores can climb out of the target.
#[test]
fn names_that_climb_or_hide_are_refused() {
    let cases: &[&[&str]] = &[
        &[".."],
        &["a", ".."],
        &["."],
        &["/etc"],
        &["a\\..\\..\\b"],
        &["C:"],
        &["file.txt:stream"],
        &["con"],
        &["trailing."],
        &["trailing "],
        &[""],
        &[],
    ];
    for bad in cases {
        assert!(below(Path::new("/t"), bad).is_err(), "{bad:?}");
    }
}

#[test]
fn parts_reads_plain_components_only() {
    assert_eq!(parts(Path::new("a/b.txt")), Some(vec!["a", "b.txt"]));
    assert_eq!(parts(Path::new("../b")), None);
    assert_eq!(parts(Path::new("/b")), None);
}

/// RC-110-92: the names the archive view lists on Windows land below the target, for an
/// unpacking (`below`) and a copy out of the archive view (`check_listed`, `check_target`) alike -
/// a `C:` or a stream in the middle of a stored name is renamed, never joined as a drive.
#[test]
fn names_cleaned_for_windows_land_below_the_target() {
    let cases: [(&str, &str); 6] = [
        ("a/C:/x.txt", "/t/a/C_/x.txt"),
        ("b.txt:Zone.Identifier", "/t/b.txt_Zone.Identifier"),
        ("CON", "/t/_CON"),
        ("name.", "/t/name_"),
        ("a<b", "/t/a_b"),
        ("a\\C:/d.\\x ", "/t/a/C_/d_/x_"),
    ];
    for (stored, landed) in cases {
        let names = rc_vfs::testing::archive_name_parts(stored, true);
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(
            below(Path::new("/t"), &names),
            Ok(PathBuf::from(landed)),
            "{stored}"
        );
        for name in &names {
            assert!(crate::name::check_listed(name).is_ok(), "{stored}: {name}");
        }
        let rel: PathBuf = names.iter().collect();
        assert!(crate::check_target(&rel, true).is_ok(), "{stored}");
        // Without Windows' cleaning the same name is refused, never joined.
        let kept = rc_vfs::testing::archive_name_parts(stored, false);
        let kept: Vec<&str> = kept.iter().map(String::as_str).collect();
        assert!(below(Path::new("/t"), &kept).is_err(), "{stored}");
    }
}
