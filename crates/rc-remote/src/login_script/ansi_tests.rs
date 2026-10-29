use super::*;

#[test]
fn colours_titles_and_charsets_are_removed() {
    assert_eq!(
        strip_ansi(b"\x1b[01;32muser@box\x1b[00m:~$ "),
        "user@box:~$ "
    );
    assert_eq!(strip_ansi(b"\x1b]0;title\x07# "), "# ");
    assert_eq!(strip_ansi(b"\x1b]0;title\x1b\\# "), "# ");
    assert_eq!(strip_ansi(b"\x1b(Bok\x1b=\x1b[?2004h$ "), "ok$ ");
}

#[test]
fn plain_text_comes_through_unchanged() {
    let text = "Password for user: \r\nroot@box:~# ";
    assert_eq!(strip_ansi(text.as_bytes()), text);
}

#[test]
fn an_unfinished_sequence_waits_for_the_next_block() {
    let mut stripper = AnsiStripper::default();
    assert_eq!(stripper.push(b"ro\x1b[1;3"), "ro");
    assert_eq!(stripper.push(b"1mot\x1b"), "ot");
    assert_eq!(stripper.push(b"[0m# "), "# ");
    assert_eq!(stripper.push(b"\x1b]0;tit"), "");
    assert_eq!(stripper.push(b"le\x07$"), "$");
}

#[test]
fn a_character_split_between_blocks_is_joined() {
    let word = "gr\u{fc}\u{df}e";
    let bytes = word.as_bytes();
    let mut stripper = AnsiStripper::default();
    let first = stripper.push(&bytes[..3]);
    let second = stripper.push(&bytes[3..]);
    assert_eq!(format!("{first}{second}"), word);
    assert_eq!(first, "gr");
}

#[test]
fn an_endless_string_sequence_is_dropped_not_held() {
    let mut stripper = AnsiStripper::default();
    let mut block = b"\x1b]0;".to_vec();
    block.extend(std::iter::repeat_n(b'x', MAX_PENDING + 1));
    assert_eq!(stripper.push(&block), "");
    assert!(stripper.pending.is_empty());
    assert_eq!(stripper.push(b"# "), "# ");
}
