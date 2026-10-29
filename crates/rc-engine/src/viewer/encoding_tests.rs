use super::*;

/// The text of the multi-encoding fixtures; the PowerShell one ends its lines with CRLF.
const TEXT: &str = "Gr\u{fc}\u{df}e aus K\u{f6}ln \u{2013} na\u{ef}ve caf\u{e9}\nzweite Zeile\n";

const UTF8: &[u8] = include_bytes!("../../tests/fixtures/viewer/utf8.txt");
const UTF8_BOM: &[u8] = include_bytes!("../../tests/fixtures/viewer/utf8-bom.txt");
/// What Windows PowerShell's `Out-File` writes: UTF-16 LE with a byte-order mark, CRLF.
const UTF16LE_BOM: &[u8] = include_bytes!("../../tests/fixtures/viewer/utf16le-bom.txt");
const UTF16LE: &[u8] = include_bytes!("../../tests/fixtures/viewer/utf16le.txt");
const UTF16BE_BOM: &[u8] = include_bytes!("../../tests/fixtures/viewer/utf16be-bom.txt");
const CP1252: &[u8] = include_bytes!("../../tests/fixtures/viewer/cp1252.txt");

fn whole(bytes: &[u8]) -> String {
    decode_window(bytes, 0, detect(bytes), true).text
}

#[test]
fn each_fixture_is_detected_and_decoded() {
    let cases: [(&[u8], TextEncoding, u64); 5] = [
        (UTF8, TextEncoding::Utf8, 0),
        (UTF8_BOM, TextEncoding::Utf8, 3),
        (UTF16LE_BOM, TextEncoding::Utf16Le, 2),
        (UTF16LE, TextEncoding::Utf16Le, 0),
        (UTF16BE_BOM, TextEncoding::Utf16Be, 2),
    ];
    for (bytes, encoding, bom_len) in cases {
        assert_eq!(
            detect(bytes),
            Detected { encoding, bom_len },
            "{encoding:?}"
        );
        let text = whole(bytes).replace("\r\n", "\n");
        assert_eq!(text, TEXT, "{encoding:?}");
    }
}

#[test]
fn invalid_utf8_falls_back_to_windows_1252() {
    assert_eq!(detect(CP1252).encoding, TextEncoding::Windows1252);
    assert_eq!(
        whole(CP1252),
        "Gr\u{fc}\u{df}e \u{20ac} 5 \u{201c}quoted\u{201d} \u{2026}\n"
    );
}

#[test]
fn ascii_and_an_empty_file_are_utf8() {
    assert_eq!(detect(b"plain text\n").encoding, TextEncoding::Utf8);
    assert_eq!(detect(b"").encoding, TextEncoding::Utf8);
}

#[test]
fn a_head_that_cuts_a_character_is_still_utf8() {
    // A full head may end inside a character the file goes on with.
    let mut head = vec![b'a'; DETECT_BYTES - 1];
    head.push(0xC3);
    assert_eq!(detect(&head).encoding, TextEncoding::Utf8);
    // A shorter head is the whole file, and so is what `detect_whole` sees: a character cut
    // off there is not UTF-8 - "caf\xE9" is Windows-1252 without a line end.
    assert_eq!(detect(b"caf\xE9").encoding, TextEncoding::Windows1252);
    assert_eq!(detect_whole(&head).encoding, TextEncoding::Windows1252);
    assert_eq!(decode_text(b"caf\xE9").1, "caf\u{e9}");
}

#[test]
fn decode_text_skips_the_mark_and_reads_utf16_without_one() {
    let cases: [(&[u8], TextEncoding, u64); 6] = [
        (UTF8, TextEncoding::Utf8, 0),
        (UTF8_BOM, TextEncoding::Utf8, 3),
        (UTF16LE_BOM, TextEncoding::Utf16Le, 2),
        (UTF16LE, TextEncoding::Utf16Le, 0),
        (UTF16BE_BOM, TextEncoding::Utf16Be, 2),
        (CP1252, TextEncoding::Windows1252, 0),
    ];
    for (bytes, encoding, bom_len) in cases {
        let (detected, text) = decode_text(bytes);
        assert_eq!(detected, Detected { encoding, bom_len }, "{encoding:?}");
        assert_eq!(text, whole(bytes), "{encoding:?}");
    }
}

#[test]
fn latin1_is_chosen_never_detected_and_maps_bytes_to_code_points() {
    let detected = Detected::chosen(TextEncoding::Latin1, CP1252);
    assert_eq!(detected.bom_len, 0);
    let text = decode_window(CP1252, 0, detected, true).text;
    assert!(
        text.starts_with("Gr\u{fc}\u{df}e \u{80} 5 \u{93}"),
        "{text:?}"
    );
}

#[test]
fn a_chosen_encoding_skips_only_its_own_mark() {
    assert_eq!(
        Detected::chosen(TextEncoding::Utf16Le, UTF16LE_BOM).bom_len,
        2
    );
    assert_eq!(Detected::chosen(TextEncoding::Utf8, UTF16LE_BOM).bom_len, 0);
}

#[test]
fn a_window_never_shows_the_mark_or_half_a_character() {
    let detected = detect(UTF8_BOM);
    // From inside the mark, ending inside the two bytes of the u-umlaut.
    let decoded = decode_window(&UTF8_BOM[1..6], 1, detected, false);
    assert_eq!(decoded.text, "Gr");
    assert_eq!((decoded.start, decoded.end), (3, 5));
    // Starting on the second byte of the u-umlaut.
    let decoded = decode_window(&UTF8_BOM[6..10], 6, detected, false);
    assert_eq!(decoded.text, "\u{df}e");
    assert_eq!((decoded.start, decoded.end), (7, 10));
}

#[test]
fn a_utf16_window_keeps_to_code_units() {
    let detected = detect(UTF16LE_BOM);
    // Offset 3 is the second byte of "G"; the window ends half way into "r".
    let decoded = decode_window(&UTF16LE_BOM[3..7], 3, detected, false);
    assert_eq!(decoded.text, "r");
    assert_eq!((decoded.start, decoded.end), (4, 6));
}

#[test]
fn a_surrogate_pair_is_never_split() {
    let mut bytes = Vec::new();
    for unit in "a\u{1F600}b".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let detected = Detected {
        encoding: TextEncoding::Utf16Le,
        bom_len: 0,
    };
    let head = decode_window(&bytes[..4], 0, detected, false);
    assert_eq!((head.text.as_str(), head.end), ("a", 2));
    let tail = decode_window(&bytes[4..], 4, detected, true);
    assert_eq!((tail.text.as_str(), tail.start), ("b", 6));
}

#[test]
fn a_cut_character_at_the_end_of_the_file_shows_as_a_replacement() {
    let decoded = decode_window(b"ab\xC3", 0, detect(b"ab"), true);
    assert_eq!(decoded.text, "ab\u{fffd}");
    assert_eq!(decoded.end, 3);
}

#[test]
fn every_windows_1252_character_encodes_back_to_its_byte() {
    for byte in 0..=255u8 {
        let text = decode(&[byte], TextEncoding::Windows1252);
        let c = text.chars().next().expect("one character");
        assert_eq!(
            encode_char(c, TextEncoding::Windows1252),
            Some(vec![byte]),
            "{byte:#x}"
        );
    }
    assert_eq!(encode_char('\u{3b1}', TextEncoding::Windows1252), None);
    assert_eq!(encode_char('\u{100}', TextEncoding::Latin1), None);
    assert_eq!(
        encode_char('\u{e9}', TextEncoding::Utf16Be),
        Some(vec![0x00, 0xE9])
    );
}

/// Windows-1252's 0x80..0x9F as the viewer's own table had them before `encoding_rs` took over;
/// the five unassigned bytes keep their C1 code point.
const OLD_CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

fn old_cp1252_char(byte: u8) -> char {
    match byte {
        0x80..=0x9F => OLD_CP1252_HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

#[test]
fn encoding_rs_windows_1252_matches_the_old_table_for_every_byte_and_character() {
    for byte in 0..=255u8 {
        let expected = old_cp1252_char(byte).to_string();
        assert_eq!(
            decode(&[byte], TextEncoding::Windows1252),
            expected,
            "{byte:#x}"
        );
    }
    // Encoding back: every character the old table knew and none it did not.
    for c in ('\0'..='\u{2200}').chain(['\u{fffd}', '\u{1F600}']) {
        let old = (0..=255u8)
            .find(|&byte| old_cp1252_char(byte) == c)
            .map(|byte| vec![byte]);
        assert_eq!(encode_char(c, TextEncoding::Windows1252), old, "{c:?}");
    }
}

#[test]
fn bytes_0x80_to_0x9f_are_windows_1252_when_detected_and_latin1_only_when_chosen() {
    let bytes = b"\x80\x93\x94\x9f";
    let (detected, text) = decode_text(bytes);
    assert_eq!(detected.encoding, TextEncoding::Windows1252);
    assert_eq!(text, "\u{20ac}\u{201c}\u{201d}\u{178}");
    assert_eq!(
        decode(bytes, TextEncoding::Latin1),
        "\u{80}\u{93}\u{94}\u{9f}"
    );
}

/// The compare window's text of `bytes`, its lines joined back with their endings.
fn compared(bytes: &[u8]) -> String {
    use crate::compare::text::{self, LineEnding};
    let file = text::decode(bytes).expect("text");
    file.lines
        .iter()
        .map(|line| {
            let ending = match line.ending {
                LineEnding::None => "",
                LineEnding::Lf => "\n",
                LineEnding::CrLf => "\r\n",
                LineEnding::Cr => "\r",
            };
            format!("{}{ending}", line.text)
        })
        .collect()
}

#[test]
fn every_caller_reads_the_same_bytes_the_same() {
    let ansi_c1: &[u8] = b"\x80 \x93quoted\x94 caf\xE9";
    let utf16be: Vec<u8> = TEXT.encode_utf16().flat_map(u16::to_be_bytes).collect();
    let cases: [&[u8]; 8] = [
        UTF8,
        UTF8_BOM,
        UTF16LE_BOM,
        UTF16LE,
        UTF16BE_BOM,
        &utf16be,
        CP1252,
        ansi_c1,
    ];
    for bytes in cases {
        let viewer = whole(bytes);
        let (_, shared) = decode_text(bytes);
        assert_eq!(shared, viewer, "decode_text {bytes:?}");
        assert_eq!(compared(bytes), viewer, "compare {bytes:?}");
        assert_eq!(
            crate::comments::decode(bytes),
            viewer,
            "descript.ion {bytes:?}"
        );
        assert_eq!(
            crate::import::tc_ini::decode(bytes).as_str(),
            viewer,
            "wincmd.ini {bytes:?}"
        );
    }
}

/// The top of an `.nfo` frame in CP437: a double-line box, a block and "Gr\u{fc}\u{df}e".
const NFO: &[u8] = b"\xC9\xCD\xCD\xBB\r\n\xBA \xDB Gr\x81\xE1e \xBA\r\n";

#[test]
fn cp437_is_chosen_never_detected_and_decodes_box_drawing() {
    assert_eq!(detect(NFO).encoding, TextEncoding::Windows1252);
    let text = decode(NFO, TextEncoding::Cp437);
    assert_eq!(
        text,
        "\u{2554}\u{2550}\u{2550}\u{2557}\r\n\u{2551} \u{2588} Gr\u{fc}\u{df}e \u{2551}\r\n"
    );
    let window = decode_window(NFO, 0, Detected::chosen(TextEncoding::Cp437, NFO), false);
    assert_eq!(window.text, text, "a single-byte window cuts nothing");
    assert_eq!(
        encode_char('\u{2550}', TextEncoding::Cp437),
        Some(vec![0xCD])
    );
    assert!(TextEncoding::ALL.contains(&TextEncoding::Cp437));
    assert_eq!(TextEncoding::Cp437.label(), "CP437 (DOS)");
}

#[test]
fn a_dos_file_is_cp437_unless_its_bytes_say_otherwise() {
    let cp437 = Detected {
        encoding: TextEncoding::Cp437,
        bom_len: 0,
    };
    assert_eq!(detect_dos(NFO), cp437, "Windows-1252 becomes CP437");
    assert_eq!(detect_dos(b"plain ascii\r\n"), cp437, "so does ASCII");
    assert_eq!(detect_dos(b""), cp437, "and an empty file");
    assert_eq!(detect_dos(UTF8).encoding, TextEncoding::Utf8, "real UTF-8");
    assert_eq!(detect_dos(UTF8_BOM), detect(UTF8_BOM), "a mark");
    assert_eq!(detect_dos(UTF16LE).encoding, TextEncoding::Utf16Le);
}
