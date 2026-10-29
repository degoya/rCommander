use super::*;
use crate::viewer::test_support::{file, open};

/// Bytes 0 to 255, then "Hello, World!\n".
const BYTES: &[u8] = include_bytes!("../../tests/fixtures/viewer/bytes.bin");

/// What `xxd bytes.bin` prints for the fixture's rows 0, 2, 7, 15 and 16 (the short last one).
const XXD: [(usize, &str); 5] = [
    (
        0,
        "00000000: 0001 0203 0405 0607 0809 0a0b 0c0d 0e0f  ................",
    ),
    (
        2,
        "00000020: 2021 2223 2425 2627 2829 2a2b 2c2d 2e2f   !\"#$%&'()*+,-./",
    ),
    (
        7,
        "00000070: 7071 7273 7475 7677 7879 7a7b 7c7d 7e7f  pqrstuvwxyz{|}~.",
    ),
    (
        15,
        "000000f0: f0f1 f2f3 f4f5 f6f7 f8f9 fafb fcfd feff  ................",
    ),
    (
        16,
        "00000100: 4865 6c6c 6f2c 2057 6f72 6c64 210a       Hello, World!.",
    ),
];

#[test]
fn rows_print_as_xxd_prints_them() {
    let rows = rows(BYTES, 0, BYTES_PER_ROW);
    assert_eq!(rows.len(), 17);
    for (index, line) in XXD {
        assert_eq!(rows[index].to_xxd(BYTES_PER_ROW), line, "row {index}");
    }
}

#[tokio::test]
async fn going_to_an_offset_shows_its_row_as_xxd_does() {
    let dir = tempfile::tempdir().expect("tempdir");
    let window = open(&file(dir.path(), "bytes.bin", BYTES)).await;
    // `xxd -s 0x75` starts its rows at 0x75; the view starts the row holding it.
    let page = read_rows(&window, 0x75, 3, BYTES_PER_ROW)
        .await
        .expect("read");
    assert_eq!(page[0].offset, 0x70);
    assert_eq!(page[0].to_xxd(BYTES_PER_ROW), XXD[2].1);
    let last = read_rows(&window, 0x10A, 4, BYTES_PER_ROW)
        .await
        .expect("read");
    assert_eq!(last.len(), 1, "the file ends in its row");
    assert_eq!(last[0].to_xxd(BYTES_PER_ROW), XXD[4].1);
}

#[test]
fn an_unaligned_start_numbers_rows_from_it() {
    let rows = rows(&BYTES[5..40], 5, BYTES_PER_ROW);
    assert_eq!(rows[0].offset, 5);
    assert_eq!(rows[1].offset, 21);
    assert_eq!(rows[2].bytes.len(), 3);
    assert_eq!(row_start(0x75, BYTES_PER_ROW), 0x70);
}

#[test]
fn offsets_are_typed_in_hex_or_decimal() {
    assert_eq!(parse_offset("0x1F"), Some(31));
    assert_eq!(parse_offset(" $ff "), Some(255));
    assert_eq!(parse_offset("10h"), Some(16));
    assert_eq!(parse_offset("1234"), Some(1234));
    assert_eq!(parse_offset("0x"), None);
    assert_eq!(parse_offset("12g"), None);
    assert_eq!(parse_offset(""), None);
}
