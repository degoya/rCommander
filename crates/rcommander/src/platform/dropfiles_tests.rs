use super::*;

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// A header with `start` as pFiles and `wide` as fWide, then `units` as the list.
fn block(start: u32, wide: u32, units: &[u16]) -> Vec<u8> {
    let mut bytes = start.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0; 12]);
    bytes.extend_from_slice(&wide.to_le_bytes());
    for unit in units {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

#[test]
fn the_header_points_past_itself_and_says_wide() {
    let bytes = encode(&[utf16("C:\\a.txt")]);
    assert_eq!(&bytes[..4], &20_u32.to_le_bytes());
    assert_eq!(&bytes[4..16], &[0; 12]);
    assert_eq!(&bytes[16..20], &1_u32.to_le_bytes());
    // "C:\a.txt" is 8 units, its NUL and the list's NUL: 10 units.
    assert_eq!(bytes.len(), HEADER_LEN + 10 * 2);
    assert_eq!(&bytes[bytes.len() - 4..], &[0; 4]);
}

#[test]
fn paths_go_through_in_order() {
    let paths = vec![
        utf16("C:\\data\\a.txt"),
        utf16("D:\\\u{4e2d}\\b"),
        utf16("\\\\server\\share\\c"),
    ];
    assert_eq!(decode(&encode(&paths)), Some(paths));
}

#[test]
fn an_empty_list_is_one_closing_nul() {
    let bytes = encode(&[]);
    assert_eq!(bytes.len(), HEADER_LEN + 2);
    assert_eq!(decode(&bytes), Some(Vec::new()));
}

#[test]
fn a_path_that_cannot_be_written_is_left_out() {
    let bytes = encode(&[Vec::new(), vec![0x41, 0, 0x42], utf16("C:\\ok")]);
    assert_eq!(decode(&bytes), Some(vec![utf16("C:\\ok")]));
}

#[test]
fn an_ansi_list_is_not_read() {
    assert_eq!(decode(&block(20, 0, &[0x4141, 0, 0])), None);
}

#[test]
fn a_header_outside_the_block_is_not_read() {
    assert_eq!(decode(&[0; 12]), None, "shorter than the header");
    assert_eq!(
        decode(&block(4, 1, &[0x41, 0, 0])),
        None,
        "inside the header"
    );
    assert_eq!(decode(&block(4096, 1, &[0x41, 0, 0])), None, "past the end");
}

#[test]
fn an_unterminated_path_is_not_read() {
    let a = u16::from(b'a');
    assert_eq!(decode(&block(20, 1, &[a, a])), None);
    assert_eq!(decode(&block(20, 1, &[a, 0, a])), None);
    // Every path ended; only the list's own NUL is missing.
    assert_eq!(decode(&block(20, 1, &[a, 0])), Some(vec![vec![a]]));
}

#[test]
fn what_follows_the_list_is_ignored() {
    let a = u16::from(b'a');
    assert_eq!(
        decode(&block(20, 1, &[a, 0, 0, 0x7777, 0x7777])),
        Some(vec![vec![a]])
    );
}

#[test]
fn an_overlong_path_or_list_is_not_read() {
    let long = vec![u16::from(b'x'); MAX_PATH_UNITS + 1];
    assert_eq!(decode(&encode(&[long])), None);
    let many = vec![utf16("C:\\f"); MAX_PATHS + 1];
    assert_eq!(decode(&encode(&many)), None);
    let most = vec![utf16("C:\\f"); MAX_PATHS];
    assert_eq!(
        decode(&encode(&most)).map(|paths| paths.len()),
        Some(MAX_PATHS)
    );
}

#[test]
fn the_preferred_effect_tells_cut_from_copy() {
    assert!(is_move(&effect_bytes(true)));
    assert!(!is_move(&effect_bytes(false)));
    // The Explorer's Ctrl+C: copy and link.
    assert!(!is_move(&5_u32.to_le_bytes()));
    assert!(!is_move(&[2, 0]), "a short block is a copy");
}
