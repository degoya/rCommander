//! Names Windows cannot hold, cleaned as 7-Zip unpacks them there (RC-110-92) - tested with
//! `windows = true` on every system.

use std::{
    ffi::OsStr,
    path::{Component, Path},
};

use proptest::prelude::*;

use super::super::{CleanName, UnsafeName, clean_name, clean_name_for};
use super::{RESERVED_CHARS, holdable, is_device};

#[test]
fn a_part_windows_cannot_hold_is_renamed_as_seven_zip_renames_it() {
    let cases = [
        ("C:", "C_"),
        ("x.exe:Zone.Identifier", "x.exe_Zone.Identifier"),
        ("a<b", "a_b"),
        ("q?*|\">", "q_____"),
        ("tab\there\u{7f}", "tab_here_"),
        ("CON", "_CON"),
        ("nul.txt", "_nul.txt"),
        ("com1.tar.gz", "_com1.tar.gz"),
        ("Lpt\u{b9}", "_Lpt\u{b9}"),
        ("con .txt", "_con .txt"),
        ("name.", "name_"),
        ("name. .", "name___"),
        ("trailing ", "trailing_"),
        // The dot at the end goes first: `CON_` is no device.
        ("CON.", "CON_"),
    ];
    for (part, held) in cases {
        assert_eq!(holdable(part).as_deref(), Some(held), "{part:?}");
    }
}

#[test]
fn a_part_windows_holds_stays() {
    for part in [
        "plain.txt",
        "console",
        "COM10",
        "lpt",
        "_CON",
        ".hidden",
        "a b",
        "\u{e4}rger",
    ] {
        assert_eq!(holdable(part), None, "{part:?}");
    }
}

/// The job's cases: a drive or a stream in the middle of a name, a device, a trailing dot, a
/// refused character, mixed separators - each part one name below the one before.
#[test]
fn names_are_cleaned_for_windows_part_by_part() {
    let cases: [(&str, Vec<&str>, Option<UnsafeName>); 8] = [
        ("a/C:/x", vec!["a", "C_", "x"], None),
        ("a/C:/x.txt", vec!["a", "C_", "x.txt"], None),
        ("b.txt:Zone.Identifier", vec!["b.txt_Zone.Identifier"], None),
        ("CON", vec!["_CON"], None),
        ("dir./name.", vec!["dir_", "name_"], None),
        ("a\\b<c/d:e\\f", vec!["a", "b_c", "d_e", "f"], None),
        ("C:/a:b", vec!["a_b"], Some(UnsafeName::DriveLetter)),
        ("../nul/x ", vec!["_nul", "x_"], Some(UnsafeName::ParentDir)),
    ];
    for (name, parts, issue) in cases {
        let clean = clean_name_for(name, true);
        assert_eq!(clean.parts, parts, "{name}");
        assert_eq!(clean.issue, issue, "{name}");
    }
}

/// Linux and macOS hold `:`, `con` and a trailing dot: there the parts stay as stored.
#[test]
fn other_systems_keep_the_parts() {
    let clean = clean_name_for("a/C:/x.exe:Zone.Identifier/con.", false);
    assert_eq!(clean.parts, ["a", "C:", "x.exe:Zone.Identifier", "con."]);
    assert_eq!(clean.issue, None);
}

/// What every part cleaned for Windows must be: one plain name, no prefix, root or stream.
fn assert_holdable(name: &str, clean: &CleanName) -> std::result::Result<(), TestCaseError> {
    for part in &clean.parts {
        let components: Vec<Component<'_>> = Path::new(part).components().collect();
        prop_assert_eq!(
            components,
            vec![Component::Normal(OsStr::new(part))],
            "{:?}",
            name
        );
        prop_assert!(part != "." && part != "..", "{:?}", name);
        prop_assert!(!part.contains(['/', '\\']), "{:?}", name);
        prop_assert!(!part.contains(RESERVED_CHARS), "{:?}", name);
        prop_assert!(!part.chars().any(char::is_control), "{:?}", name);
        prop_assert!(!part.ends_with(['.', ' ']), "{:?}", name);
        prop_assert!(!is_device(part), "{:?}", name);
    }
    Ok(())
}

/// Pieces a name is made of below: what Windows reads as a drive, a stream or a device.
const PIECES: &[&str] = &[
    "a",
    "Zz",
    "C:",
    ":",
    "x.exe:Zone.Identifier",
    "con",
    "NUL.txt",
    "com1",
    "lpt\u{b2}",
    ".",
    "..",
    " ",
    "<",
    ">",
    "\"",
    "|",
    "?",
    "*",
    "\u{0}",
    "\t",
    "/",
    "\\",
    "//",
    "\\\\",
];

proptest! {
    /// Any name: each part cleaned for Windows is one plain name Windows holds, the parts are
    /// those of the other systems one for one and the issue the same, and the parts joined again
    /// clean to themselves.
    #[test]
    fn any_name_cleans_to_parts_windows_holds(name in any::<String>()) {
        check(&name)?;
    }

    /// Names made of the pieces Windows reads as drives, streams, devices and separators.
    #[test]
    fn names_of_windows_pieces_clean_to_parts_windows_holds(
        pieces in prop::collection::vec(prop::sample::select(PIECES), 0..10),
    ) {
        check(&pieces.concat())?;
    }
}

fn check(name: &str) -> std::result::Result<(), TestCaseError> {
    let clean = clean_name_for(name, true);
    assert_holdable(name, &clean)?;
    let elsewhere = clean_name(name);
    prop_assert_eq!(clean.parts.len(), elsewhere.parts.len(), "{:?}", name);
    prop_assert_eq!(clean.issue, elsewhere.issue, "{:?}", name);
    let again = clean_name_for(&clean.parts.join("/"), true);
    prop_assert_eq!(&again.parts, &clean.parts, "{:?}", name);
    prop_assert_eq!(again.issue, None, "{:?}", name);
    Ok(())
}
