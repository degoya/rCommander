use super::*;

fn text(content: &str) -> TextFile {
    decode(content.as_bytes()).expect("text")
}

fn diff(left: &str, right: &str, options: TextOptions) -> TextDiff {
    TextDiff::compute(text(left), text(right), &options)
}

fn kinds(diff: &TextDiff) -> Vec<RowKind> {
    diff.rows.iter().map(|row| row.kind).collect()
}

#[test]
fn encodings_are_recognised_by_mark_then_utf8_then_windows_1252() {
    assert_eq!(decode(b"a\n").expect("text").encoding, Encoding::Utf8);
    let bom = decode(b"\xEF\xBB\xBFgr\xC3\xBC\xC3\x9F").expect("text");
    assert_eq!(bom.encoding, Encoding::Utf8Bom);
    assert_eq!(bom.lines[0].text, "gr\u{fc}\u{df}");
    let ansi = decode(b"gr\xFC\xDF").expect("text");
    assert_eq!(ansi.encoding, Encoding::Windows1252);
    assert_eq!(ansi.lines[0].text, "gr\u{fc}\u{df}");
    let utf16 = decode(b"\xFF\xFEh\0i\0\n\0").expect("text");
    assert_eq!(utf16.encoding, Encoding::Utf16Le);
    assert_eq!(utf16.lines[0].text, "hi");
    let utf16be = decode(b"\xFE\xFF\0h\0i").expect("text");
    assert_eq!(utf16be.encoding, Encoding::Utf16Be);
    assert_eq!(utf16be.lines[0].text, "hi");
}

#[test]
fn a_nul_byte_makes_a_file_binary() {
    assert_eq!(decode(b"MZ\0\x03"), None);
}

#[test]
fn utf16_without_a_mark_is_text_now() {
    let bytes: Vec<u8> = "first line\nsecond\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let file = decode(&bytes).expect("UTF-16 LE without a mark is text, not binary");
    assert_eq!(file.encoding, Encoding::Utf16Le);
    assert_eq!(file.lines[1].text, "second");
}

#[test]
fn bytes_0x80_to_0x9f_are_windows_1252_not_latin1() {
    // The euro sign and typographic quotes, which Latin-1 read as C1 control characters.
    let file = decode(b"\x80 \x93quoted\x94").expect("text");
    assert_eq!(file.encoding, Encoding::Windows1252);
    assert_eq!(file.lines[0].text, "\u{20ac} \u{201c}quoted\u{201d}");
}

#[test]
fn lines_keep_their_endings() {
    let file = text("a\r\nb\nc\rd");
    let endings: Vec<LineEnding> = file.lines.iter().map(|line| line.ending).collect();
    assert_eq!(
        endings,
        [
            LineEnding::CrLf,
            LineEnding::Lf,
            LineEnding::Cr,
            LineEnding::None
        ]
    );
    let texts: Vec<&str> = file.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(texts, ["a", "b", "c", "d"]);
    assert!(text("").lines.is_empty());
}

#[test]
fn equal_texts_have_no_changes() {
    let same = diff("a\nb\n", "a\nb\n", TextOptions::default());
    assert!(same.is_equal());
    assert_eq!(same.changes(), 0);
    assert_eq!(same.next_change(None), None);
}

#[test]
fn a_replaced_block_pairs_its_lines_and_the_rest_is_inserted() {
    let changed = diff("a\nb\nc\n", "a\nB\nB2\nc\n", TextOptions::default());
    assert_eq!(
        kinds(&changed),
        [
            RowKind::Equal,
            RowKind::Changed,
            RowKind::Inserted,
            RowKind::Equal
        ]
    );
    assert_eq!(changed.rows[2].left, None);
    assert_eq!(changed.rows[2].right, Some(2));
    assert_eq!(changed.changes(), 1);
}

#[test]
fn deleted_lines_have_no_right_side() {
    let deleted = diff("a\nb\nc\n", "a\nc\n", TextOptions::default());
    assert_eq!(
        kinds(&deleted),
        [RowKind::Equal, RowKind::Deleted, RowKind::Equal]
    );
    assert_eq!(deleted.rows[1].left, Some(1));
}

#[test]
fn whitespace_counts_unless_ignored() {
    let (left, right) = ("int  x = 1;\n\tfoo()\n", "int x=1;\nfoo()\n");
    assert_eq!(diff(left, right, TextOptions::default()).changes(), 1);
    let ignoring = TextOptions {
        ignore_whitespace: true,
        ..TextOptions::default()
    };
    assert!(diff(left, right, ignoring).is_equal());
}

#[test]
fn case_counts_unless_ignored() {
    assert!(!diff("Hello\n", "hello\n", TextOptions::default()).is_equal());
    let ignoring = TextOptions {
        ignore_case: true,
        ..TextOptions::default()
    };
    assert!(diff("Hello\n", "hELLO\n", ignoring).is_equal());
}

#[test]
fn line_endings_count_unless_ignored() {
    let (unix, windows) = ("a\nb\n", "a\r\nb\r\n");
    let strict = diff(unix, windows, TextOptions::default());
    assert_eq!(kinds(&strict), [RowKind::Changed, RowKind::Changed]);
    let ignoring = TextOptions {
        ignore_line_endings: true,
        ..TextOptions::default()
    };
    assert!(diff(unix, windows, ignoring).is_equal());
    // The last line without an ending is an ending difference too.
    assert!(!diff("a\n", "a", TextOptions::default()).is_equal());
    assert!(diff("a\n", "a", ignoring).is_equal());
}

#[test]
fn jumps_go_from_block_to_block() {
    let jumps = diff(
        "a\nb\nc\nd\ne\n",
        "a\nX\nc\nd\nY\nZ\n",
        TextOptions::default(),
    );
    // Rows: a = | b/X changed | c = | d = | e/Y changed | Z inserted
    assert_eq!(jumps.change_starts().collect::<Vec<_>>(), [1, 4]);
    assert_eq!(jumps.next_change(None), Some(1));
    assert_eq!(jumps.next_change(Some(1)), Some(4));
    assert_eq!(jumps.next_change(Some(4)), None);
    assert_eq!(jumps.previous_change(5), Some(4));
    assert_eq!(jumps.previous_change(4), Some(1));
    assert_eq!(jumps.previous_change(1), None);
    assert_eq!(jumps.position(4), LinePosition { left: 5, right: 5 });
}

#[test]
fn the_unified_diff_has_hunks_with_context() {
    let left = "1\n2\n3\n4\n5\n6\n7\n8\n9\n";
    let right = "1\n2\n3\n4\nfive\n6\n7\n8\n9\n";
    let unified = diff(left, right, TextOptions::default()).unified(2, 10_000);
    assert!(!unified.truncated);
    assert_eq!(unified.text, "@@ -3,5 +3,5 @@\n 3\n 4\n-5\n+five\n 6\n 7\n");
}

#[test]
fn an_insertion_at_the_start_names_line_zero() {
    let unified = diff("a\n", "new\na\n", TextOptions::default()).unified(0, 10_000);
    assert_eq!(unified.text, "@@ -0,0 +1,1 @@\n+new\n");
}

#[test]
fn the_unified_diff_stops_at_the_limit() {
    let left: String = (0..100).map(|i| format!("line {i}\n")).collect();
    let right: String = (0..100).map(|i| format!("LINE {i}\n")).collect();
    let full = diff(&left, &right, TextOptions::default()).unified(0, usize::MAX);
    let cut = diff(&left, &right, TextOptions::default()).unified(0, 50);
    assert!(!full.truncated);
    assert!(cut.truncated);
    assert!(cut.text.len() <= 50);
}

/// A file of nothing but line breaks is cut off before it costs a line each: from
/// [`MAX_LINES`] breaks on it is not compared line by line.
#[test]
fn a_file_of_too_many_lines_is_not_read_as_text() {
    assert_eq!(decode(&vec![b'\n'; MAX_LINES]), None);
    assert_eq!(decode(&vec![b'\r'; MAX_LINES]), None);
    let below = decode(&vec![b'\n'; MAX_LINES - 1]).expect("text");
    assert_eq!(below.lines.len(), MAX_LINES - 1);
}
