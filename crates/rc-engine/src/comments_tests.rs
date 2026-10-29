use super::{Descriptions, decode, encode};

#[test]
fn names_plain_and_quoted_are_found_ignoring_case() {
    let text = "readme.txt The first file\r\n\"My Notes.md\" notes with spaces\r\nlonely\r\n";
    let comments = Descriptions::parse(text);
    assert_eq!(
        comments.get("README.TXT").as_deref(),
        Some("The first file")
    );
    assert_eq!(
        comments.get("my notes.md").as_deref(),
        Some("notes with spaces")
    );
    assert_eq!(comments.get("lonely").as_deref(), Some(""));
    assert_eq!(comments.get("missing"), None);
}

#[test]
fn set_replaces_adds_and_removes_and_keeps_the_rest() {
    let mut comments = Descriptions::parse("a.txt one\r\nb.txt two\r\n");
    comments.set("A.TXT", Some("uno"));
    comments.set("new file", Some("added"));
    comments.set("b.txt", None);
    assert_eq!(comments.render(), "A.TXT uno\r\n\"new file\" added\r\n");
    comments.set("A.TXT", Some("  "));
    comments.set("new file", None);
    assert!(comments.is_empty());
}

#[test]
fn several_lines_round_trip_as_total_commander_writes_them() {
    let mut comments = Descriptions::parse("");
    comments.set("f", Some("line one\nline two with \\ backslash"));
    let text = comments.render();
    assert_eq!(
        text,
        "f line one\\nline two with \\\\ backslash\u{4}\u{C2}\r\n"
    );
    let back = Descriptions::parse(&text);
    assert_eq!(
        back.get("f").as_deref(),
        Some("line one\nline two with \\ backslash")
    );
}

#[test]
fn ansi_and_utf8_files_both_read() {
    // "caf\u{e9}" in Windows-1252, then as UTF-8 with and without a byte-order mark.
    assert_eq!(decode(b"x caf\xe9"), "x caf\u{e9}");
    assert_eq!(decode("x caf\u{e9}".as_bytes()), "x caf\u{e9}");
    assert_eq!(decode(&encode("x caf\u{e9}")), "x caf\u{e9}");
    assert_eq!(encode("plain"), b"plain");
    assert!(encode("caf\u{e9}").starts_with(&[0xEF, 0xBB, 0xBF]));
}
