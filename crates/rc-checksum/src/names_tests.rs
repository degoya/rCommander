use std::path::Path;

use super::{inside, listed, plain};

#[test]
fn a_plain_name_is_one_file_name() {
    assert_eq!(plain("file.iso").expect("plain"), "file.iso");
    assert_eq!(plain("with space.txt").expect("plain"), "with space.txt");
    for bad in ["", ".", "..", "a/b", "a\\b", "C:x", "a\nb"] {
        let error = plain(bad).expect_err(bad);
        assert_eq!(error.code(), "invalid_name", "{bad:?}");
    }
}

/// RC-080-43 (S9): a name this crate creates keeps the one name rule - no device, no character
/// Windows refuses, no trailing dot, no turned reading direction.
#[test]
fn a_created_name_keeps_the_one_name_rule() {
    for bad in [
        "CON",
        "nul",
        "con.iso",
        "a.",
        "a ",
        "a<b",
        "a|b",
        "a?b",
        "a*b",
        "a\"b",
        "invoice\u{202e}fdp.exe",
    ] {
        let error = plain(bad).expect_err(bad);
        assert_eq!(error.code(), "invalid_name", "{bad:?}");
    }
}

/// An entry to hash only has to be one name a line can carry and read back.
#[test]
fn a_listed_name_is_one_entry_of_its_folder() {
    for good in ["file.iso", "a?b", "con", "report.", ".hidden"] {
        assert_eq!(listed(good).expect(good), good);
    }
    for bad in ["", ".", "..", "a/b", "a\\b", "x:y", "a\nb", "../x"] {
        let error = listed(bad).expect_err(bad);
        assert_eq!(error.code(), "invalid_name", "{bad:?}");
    }
}

#[test]
fn a_name_from_a_checksum_file_stays_inside_its_folder() {
    let base = Path::new("/data");
    assert_eq!(inside(base, "a.iso"), Ok(base.join("a.iso")));
    assert_eq!(inside(base, "./a.iso"), Ok(base.join("a.iso")));
    assert_eq!(
        inside(base, "sub/a.iso"),
        Ok(base.join("sub").join("a.iso"))
    );
    assert_eq!(
        inside(base, "sub\\a.iso"),
        Ok(base.join("sub").join("a.iso"))
    );
}

#[test]
fn a_name_that_leaves_its_folder_is_refused() {
    let base = Path::new("/data");
    let cases = [
        ("", "empty_name"),
        (".", "empty_name"),
        ("./", "empty_name"),
        ("/etc/passwd", "absolute_name"),
        ("\\Windows\\win.ini", "absolute_name"),
        ("\\\\server\\share\\x", "absolute_name"),
        ("\\\\?\\C:\\x", "absolute_name"),
        ("C:\\Windows\\win.ini", "forbidden_character"),
        ("C:win.ini", "forbidden_character"),
        ("file.txt:stream", "forbidden_character"),
        ("../secret", "parent_reference"),
        ("sub/../../secret", "parent_reference"),
        ("sub\\..\\..\\secret", "parent_reference"),
        ("a\u{0}b", "forbidden_character"),
    ];
    for (name, reason) in cases {
        assert_eq!(inside(base, name), Err(reason), "{name:?}");
    }
}
