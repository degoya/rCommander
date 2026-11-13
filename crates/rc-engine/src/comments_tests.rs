use super::{Descriptions, NotEncodable};

fn text(comments: &Descriptions) -> String {
    String::from_utf8(comments.to_bytes()).expect("UTF-8")
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

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
    comments.set("A.TXT", Some("uno")).expect("set");
    comments.set("new file", Some("added")).expect("set");
    comments.set("b.txt", None).expect("removed");
    assert_eq!(text(&comments), "A.TXT uno\r\n\"new file\" added\r\n");
    comments.set("A.TXT", Some("  ")).expect("removed");
    comments.set("new file", None).expect("removed");
    assert!(comments.is_empty());
}

#[test]
fn several_lines_round_trip_as_total_commander_writes_them() {
    let mut comments = Descriptions::parse("");
    comments
        .set("f", Some("line one\nline two with \\ backslash"))
        .expect("set");
    let bytes = comments.to_bytes();
    assert_eq!(
        String::from_utf8(bytes.clone()).expect("UTF-8"),
        "\u{feff}f line one\\nline two with \\\\ backslash\u{4}\u{C2}\r\n"
    );
    let back = Descriptions::from_bytes(&bytes);
    assert_eq!(
        back.get("f").as_deref(),
        Some("line one\nline two with \\ backslash")
    );
}

#[test]
fn ansi_and_utf8_files_both_read() {
    // "caf\u{e9}" in Windows-1252, then as UTF-8 with and without a byte-order mark.
    let read = |bytes: &[u8]| Descriptions::from_bytes(bytes).get("x");
    assert_eq!(read(b"x caf\xe9").as_deref(), Some("caf\u{e9}"));
    assert_eq!(read("x caf\u{e9}".as_bytes()).as_deref(), Some("caf\u{e9}"));
    assert_eq!(
        read("\u{feff}x caf\u{e9}".as_bytes()).as_deref(),
        Some("caf\u{e9}")
    );
}

/// "gr\u{f6}\u{df}e", "\u{e4}\u{fc}" and a box line as OEM 850 has them: no valid UTF-8, so the
/// file is read as Windows-1252 - and must come back as the bytes it was.
const OEM: &[u8] =
    b"alt.txt gr\x94\xe1e\r\nnotes.txt old\n\r\n\"my file.txt\" \x84\x81 \xcd\xcd\r\n";

#[test]
fn a_code_page_file_keeps_every_other_line_byte_for_byte() {
    let mut comments = Descriptions::from_bytes(OEM);
    assert_eq!(comments.to_bytes(), OEM, "read and written back unchanged");

    comments.set("notes.txt", Some("new")).expect("set");
    assert_eq!(
        comments.to_bytes(),
        b"alt.txt gr\x94\xe1e\r\nnotes.txt new\r\n\r\n\"my file.txt\" \x84\x81 \xcd\xcd\r\n"
    );
    // A new comment beyond ASCII is written as the file is read, in Windows-1252.
    comments.set("added.txt", Some("caf\u{e9}")).expect("set");
    assert_eq!(
        comments.to_bytes(),
        b"alt.txt gr\x94\xe1e\r\nnotes.txt new\r\n\r\n\"my file.txt\" \x84\x81 \xcd\xcd\r\n\
          added.txt caf\xe9\r\n"
    );
    assert_eq!(comments.get("added.txt").as_deref(), Some("caf\u{e9}"));

    comments.set("alt.txt", None).expect("removed");
    assert_eq!(
        comments.to_bytes(),
        b"notes.txt new\r\n\r\n\"my file.txt\" \x84\x81 \xcd\xcd\r\nadded.txt caf\xe9\r\n"
    );
}

#[test]
fn a_comment_the_encoding_lacks_is_refused_and_nothing_changes() {
    let mut comments = Descriptions::from_bytes(OEM);
    // Cyrillic is not in Windows-1252, in a comment or in a name.
    let refused = NotEncodable {
        encoding: "Windows-1252",
    };
    assert_eq!(comments.set("notes.txt", Some("\u{416}")), Err(refused));
    assert_eq!(comments.set("\u{416}.txt", Some("plain")), Err(refused));
    assert_eq!(comments.to_bytes(), OEM);
    assert_eq!(comments.get("notes.txt").as_deref(), Some("old"));
}

#[test]
fn utf8_keeps_its_mark_or_the_lack_of_one() {
    let without = "a.txt caf\u{e9}\nb.txt two\n";
    let mut comments = Descriptions::parse(without);
    comments.set("b.txt", Some("\u{416}")).expect("set");
    assert_eq!(text(&comments), "a.txt caf\u{e9}\nb.txt \u{416}\r\n");

    let with = "\u{feff}a.txt one\r\n";
    let mut comments = Descriptions::parse(with);
    comments.set("b.txt", Some("two")).expect("set");
    assert_eq!(text(&comments), "\u{feff}a.txt one\r\nb.txt two\r\n");
}

#[test]
fn plain_ascii_stays_ascii_until_a_comment_needs_more() {
    let mut comments = Descriptions::parse("a.txt one\nb.txt two");
    comments.set("c.txt", Some("three")).expect("set");
    // The last line had no line break: it gets one before a line follows.
    assert_eq!(text(&comments), "a.txt one\nb.txt two\r\nc.txt three\r\n");
    comments.set("a.txt", Some("\u{20ac}")).expect("set");
    assert_eq!(
        text(&comments),
        "\u{feff}a.txt \u{20ac}\r\nb.txt two\r\nc.txt three\r\n"
    );
}

#[test]
fn utf16_stays_utf16_and_changes_one_line() {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(utf16le("a.txt gr\u{f6}\u{df}e\r\nb.txt two\r\n"));
    let mut comments = Descriptions::from_bytes(&bytes);
    assert_eq!(comments.get("a.txt").as_deref(), Some("gr\u{f6}\u{df}e"));
    assert_eq!(comments.to_bytes(), bytes);

    comments
        .set("b.txt", Some("\u{416} \u{1f600}"))
        .expect("set");
    comments.set("c d", Some("three")).expect("set");
    let mut expected = vec![0xFF, 0xFE];
    expected.extend(utf16le(
        "a.txt gr\u{f6}\u{df}e\r\nb.txt \u{416} \u{1f600}\r\n\"c d\" three\r\n",
    ));
    assert_eq!(comments.to_bytes(), expected);
}

#[test]
fn lines_that_are_not_understood_and_empty_ones_stay() {
    let file = "\"broken line\r\n\r\na.txt one\r\n";
    let mut comments = Descriptions::parse(file);
    assert_eq!(comments.entries().count(), 1);
    comments.set("a.txt", None).expect("removed");
    assert!(
        !comments.is_empty(),
        "the line that is not understood is kept"
    );
    assert_eq!(text(&comments), "\"broken line\r\n\r\n");
    assert!(Descriptions::parse("\r\n \r\n").is_empty());
}
