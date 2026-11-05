use super::*;
use crate::test_support::TestSource;

/// Every line of `bytes` as its kept bytes and tail, read through windows of seven bytes.
fn lines(bytes: &[u8], cap: usize) -> Vec<(Vec<u8>, Option<u8>)> {
    let mut src = TestSource::new(bytes.to_vec());
    let mut scanner = Scanner::new(Reader::new(&mut src, 0, 7));
    let (mut raw, mut out) = (Vec::new(), Vec::new());
    while let Some(line) = scanner.next_line(cap, &mut raw).expect("read") {
        out.push((raw.clone(), line.tail));
    }
    out
}

#[test]
fn lines_without_their_break_and_the_last_byte_that_is_no_blank() {
    assert_eq!(
        lines(b"RUN a \\\r\n  b\n\nc", 100),
        [
            (b"RUN a \\\r".to_vec(), Some(b'\\')),
            (b"  b".to_vec(), Some(b'b')),
            (Vec::new(), None),
            (b"c".to_vec(), Some(b'c')),
        ]
    );
    assert!(lines(b"", 100).is_empty());
    assert_eq!(lines(b"x\n", 100), [(b"x".to_vec(), Some(b'x'))]);
}

#[test]
fn a_long_line_is_cut_but_its_tail_is_still_known() {
    assert_eq!(
        lines(b"abcdefghij \\\nnext\n", 4),
        [
            (b"abcd".to_vec(), Some(b'\\')),
            (b"next".to_vec(), Some(b't'))
        ]
    );
    // A cut inside a character moves back to the ASCII byte before it.
    let accented = "ab\u{e9}\u{e9}".as_bytes();
    assert_eq!(lines(accented, 3), [(b"ab".to_vec(), Some(0xA9))]);
}
