use super::{names_from_bytes, names_from_text, names_to_bytes};
use crate::error::RenameError;
use crate::rule::{MAX_NAMES, MAX_NAMES_BYTES};

#[test]
fn lines_are_names_in_lf_and_crlf() {
    let names = |text: &str| names_from_text(text).expect("reads");
    assert_eq!(names("a.txt\nb.txt\n"), ["a.txt", "b.txt"]);
    assert_eq!(names("a.txt\r\nb.txt\r\n"), ["a.txt", "b.txt"]);
    assert_eq!(names("a.txt\r\nb.txt"), ["a.txt", "b.txt"]);
    // An empty line is a name (an invalid one, which the plan reports).
    assert_eq!(names("a\n\nc"), ["a", "", "c"]);
    assert_eq!(names(" spaced \n"), [" spaced "]);
    assert!(names("").is_empty());
    assert!(names("\r\n").is_empty());
}

#[test]
fn a_file_is_read_in_its_encoding() {
    let names = |bytes: &[u8]| names_from_bytes(bytes).expect("reads");
    // UTF-8 with and without a byte-order mark.
    assert_eq!(names(b"\xEF\xBB\xBF\xC3\xA4.txt\r\n"), ["\u{e4}.txt"]);
    assert_eq!(names("\u{e4}.txt\n".as_bytes()), ["\u{e4}.txt"]);
    // UTF-16 LE, as Notepad saves "Unicode".
    let utf16: Vec<u8> = [0xFEFF_u16]
        .into_iter()
        .chain("\u{e4}\r\nb".encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(names(&utf16), ["\u{e4}", "b"]);
    // ANSI: not valid UTF-8, read as Windows-1252.
    assert_eq!(names(b"\xE4.txt"), ["\u{e4}.txt"]);
}

#[test]
fn a_list_is_bounded() {
    let too_large = RenameError::NameList {
        max_names: MAX_NAMES,
        max_bytes: MAX_NAMES_BYTES,
    };
    let many = "x\n".repeat(MAX_NAMES + 1);
    assert_eq!(names_from_text(&many), Err(too_large.clone()));
    assert_eq!(
        names_from_text(&"x\n".repeat(MAX_NAMES)).map(|names| names.len()),
        Ok(MAX_NAMES)
    );
    assert_eq!(
        names_from_bytes(&vec![b'x'; MAX_NAMES_BYTES + 1]),
        Err(too_large)
    );
}

#[test]
fn the_editor_gets_utf8_with_a_bom_and_reads_it_back() {
    let names = vec!["\u{e4}.txt".to_owned(), "b".to_owned()];
    let windows = names_to_bytes(&names, true);
    assert_eq!(windows, b"\xEF\xBB\xBF\xC3\xA4.txt\r\nb\r\n");
    assert_eq!(
        names_to_bytes(&names, false),
        b"\xEF\xBB\xBF\xC3\xA4.txt\nb\n"
    );
    assert_eq!(names_from_bytes(&windows), Ok(names));
}
