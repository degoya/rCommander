use super::{FsRules, InvalidName, check_name, check_plain};

const DIR: &str = r"C:\data";

#[test]
fn windows_refuses_what_ntfs_and_the_win32_api_refuse() {
    let cases = [
        ("", Some(InvalidName::Empty)),
        (".", Some(InvalidName::DotName)),
        ("..", Some(InvalidName::DotName)),
        ("a/b", Some(InvalidName::Separator)),
        (r"a\b", Some(InvalidName::Separator)),
        ("a<b", Some(InvalidName::ReservedChar { ch: '<' })),
        ("a>b", Some(InvalidName::ReservedChar { ch: '>' })),
        ("a:b", Some(InvalidName::ReservedChar { ch: ':' })),
        ("a\"b", Some(InvalidName::ReservedChar { ch: '"' })),
        ("a|b", Some(InvalidName::ReservedChar { ch: '|' })),
        ("a?b", Some(InvalidName::ReservedChar { ch: '?' })),
        ("a*b", Some(InvalidName::ReservedChar { ch: '*' })),
        ("a\tb", Some(InvalidName::ControlChar)),
        ("a\u{1f}b", Some(InvalidName::ControlChar)),
        ("CON", Some(InvalidName::ReservedName)),
        ("con.txt", Some(InvalidName::ReservedName)),
        ("NUL.tar.gz", Some(InvalidName::ReservedName)),
        ("aux ", Some(InvalidName::ReservedName)),
        ("COM1", Some(InvalidName::ReservedName)),
        ("lpt9.log", Some(InvalidName::ReservedName)),
        ("COM\u{b9}", Some(InvalidName::ReservedName)),
        ("CONIN$", Some(InvalidName::ReservedName)),
        ("name.", Some(InvalidName::TrailingDotOrSpace)),
        ("name ", Some(InvalidName::TrailingDotOrSpace)),
        ("console", None),
        ("COM10", None),
        ("CON_x.txt", None),
        ("x.CON", None),
        (".hidden", None),
        ("notes [1] {2}.txt", None),
        ("\u{e4}\u{f6}\u{fc}.txt", None),
    ];
    for (name, expected) in cases {
        assert_eq!(
            check_name(name, DIR, &FsRules::WINDOWS),
            expected,
            "{name:?}"
        );
    }
}

#[test]
fn the_windows_rules_hold_on_a_unix_file_system_too() {
    let cases = [
        ("a:b", Some(InvalidName::ReservedChar { ch: ':' })),
        ("a?b", Some(InvalidName::ReservedChar { ch: '?' })),
        ("CON", Some(InvalidName::ReservedName)),
        ("nul.txt", Some(InvalidName::ReservedName)),
        ("name.", Some(InvalidName::TrailingDotOrSpace)),
        ("name ", Some(InvalidName::TrailingDotOrSpace)),
        ("a\tb", Some(InvalidName::ControlChar)),
        ("a\0b", Some(InvalidName::ControlChar)),
        ("a/b", Some(InvalidName::Separator)),
        (r"a\b", Some(InvalidName::Separator)),
        ("..", Some(InvalidName::DotName)),
        ("", Some(InvalidName::Empty)),
        ("a..b", None),
        ("Readme", None),
    ];
    for (name, expected) in cases {
        assert_eq!(
            check_name(name, "/data", &FsRules::UNIX),
            expected,
            "{name:?}"
        );
    }
}

#[test]
fn names_and_paths_have_a_maximum_length() {
    let windows = FsRules::WINDOWS;
    // 255 UTF-16 units fit, 256 do not; a character outside the BMP takes two.
    assert_eq!(check_name(&"a".repeat(255), DIR, &windows), None);
    assert_eq!(
        check_name(&"a".repeat(256), DIR, &windows),
        Some(InvalidName::NameTooLong)
    );
    assert_eq!(
        check_name(&"\u{1f600}".repeat(128), DIR, &windows),
        Some(InvalidName::NameTooLong)
    );
    // Unix counts bytes: 128 two-byte characters are 256 bytes.
    assert_eq!(
        check_name(&"\u{e4}".repeat(128), "/d", &FsRules::UNIX),
        Some(InvalidName::NameTooLong)
    );

    let short = FsRules {
        max_path: 12,
        ..FsRules::WINDOWS
    };
    // `C:\data` + `\` + 4 characters = 12.
    assert_eq!(check_name("abcd", DIR, &short), None);
    assert_eq!(
        check_name("abcde", DIR, &short),
        Some(InvalidName::PathTooLong)
    );
    // A directory that ends in a separator adds none.
    assert_eq!(check_name("abcd", r"C:\data\", &short), None);
}

#[test]
fn the_key_folds_case_only_where_the_file_system_ignores_it() {
    assert_eq!(FsRules::WINDOWS.key("ReadMe.TXT"), "readme.txt");
    assert_eq!(FsRules::UNIX.key("ReadMe.TXT"), "ReadMe.TXT");
}

#[test]
fn every_invalid_name_has_a_listed_code() {
    let all = [
        InvalidName::Empty,
        InvalidName::DotName,
        InvalidName::Separator,
        InvalidName::ReservedChar { ch: '?' },
        InvalidName::ControlChar,
        InvalidName::ReservedName,
        InvalidName::TrailingDotOrSpace,
        InvalidName::NameTooLong,
        InvalidName::PathTooLong,
        InvalidName::ParentDir,
        InvalidName::AbsolutePath,
        InvalidName::DrivePath,
        InvalidName::UncPath,
        InvalidName::EmptyPart,
        InvalidName::TurnsDirection,
    ];
    let codes: Vec<&str> = all.iter().map(InvalidName::code).collect();
    assert_eq!(codes, InvalidName::ALL_CODES);
}

/// RC-080-43: a new name that turns the reading direction reads as another name, in the
/// multi-rename as everywhere else.
#[test]
fn a_new_name_may_not_turn_the_reading_direction() {
    for name in [
        "invoice\u{202e}fdp.exe",
        "left\u{2067}to",
        "mark\u{200f}",
        "\u{61c}x",
    ] {
        for rules in [FsRules::WINDOWS, FsRules::UNIX] {
            assert_eq!(
                check_name(name, DIR, &rules),
                Some(InvalidName::TurnsDirection),
                "{name:?}"
            );
        }
        assert_eq!(
            check_plain(name),
            Some(InvalidName::TurnsDirection),
            "{name:?}"
        );
    }
    // A zero-width joiner holds an emoji together; it turns nothing.
    assert_eq!(check_plain("family \u{1f468}\u{200d}\u{1f469}.jpg"), None);
}

/// The one plain-name rule: what `check_name` refuses, with the length in bytes and no folder.
#[test]
fn a_plain_name_keeps_the_windows_rules_and_255_bytes() {
    for (name, expected) in [
        ("report.pdf", None),
        (".hidden", None),
        ("console", None),
        ("a:b", Some(InvalidName::ReservedChar { ch: ':' })),
        ("a/b", Some(InvalidName::Separator)),
        ("con.txt", Some(InvalidName::ReservedName)),
        ("report.", Some(InvalidName::TrailingDotOrSpace)),
        ("tab\there", Some(InvalidName::ControlChar)),
        ("..", Some(InvalidName::DotName)),
        ("", Some(InvalidName::Empty)),
    ] {
        assert_eq!(check_plain(name), expected, "{name:?}");
    }
    assert_eq!(check_plain(&"x".repeat(255)), None);
    assert_eq!(
        check_plain(&"x".repeat(256)),
        Some(InvalidName::NameTooLong)
    );
    assert_eq!(
        check_plain(&"\u{e4}".repeat(128)),
        Some(InvalidName::NameTooLong)
    );
}
