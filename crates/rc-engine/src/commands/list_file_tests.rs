use super::*;

#[test]
fn utf16_with_bom_is_read() {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in "a.txt\r\nb \u{e4}.md\r\n".encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    assert_eq!(decoded(&bytes), "a.txt\r\nb \u{e4}.md\r\n");
    let mut bytes = vec![0xFE, 0xFF];
    for unit in "x".encode_utf16() {
        bytes.extend(unit.to_be_bytes());
    }
    assert_eq!(decoded(&bytes), "x");
    // No mark and no valid UTF-8: an ANSI file (a cut character at the very end would read as
    // UTF-8 still, `rc_plugin_text::detect`).
    assert_eq!(decoded(b"caf\xE9 au lait"), "caf\u{e9} au lait");
    assert_eq!(decoded("\u{fc}n\u{ef}code".as_bytes()), "\u{fc}n\u{ef}code");
}

#[test]
fn lines_are_trimmed_and_unquoted() {
    let lines = list_lines("  a.txt \r\n\"with space.txt\"\n\n\"\"\nlast").expect("lines");
    assert_eq!(lines, ["a.txt", "with space.txt", "last"]);
}

#[test]
fn too_many_lines_are_refused() {
    let text = "x\n".repeat(MAX_LIST_LINES + 1);
    let error = list_lines(&text).expect_err("too many");
    assert_eq!(error.code(), "list_file_too_large");
    assert_eq!(list_lines(&"x\n".repeat(10)).expect("fits").len(), 10);
}
