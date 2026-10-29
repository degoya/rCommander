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
