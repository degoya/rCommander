use super::*;
use crate::test_support::{TestSource, VecSink};

/// Text with a stray byte, a sequence cut short in the middle and one at the end.
fn mixed() -> Vec<u8> {
    let mut bytes = b"ok \xFF \xE2\x82 x ".to_vec();
    bytes.extend_from_slice("\u{e9}\u{20ac}\u{1f600}".as_bytes());
    bytes.push(0xE2);
    bytes
}

#[test]
fn the_text_replaces_what_is_not_utf8_as_from_utf8_lossy_does() {
    let bytes = mixed();
    let expected = String::from_utf8_lossy(&bytes).into_owned();
    // Windows of a byte or two cut every sequence; the carried bytes join them again.
    for chunk in [1, 2, 3, 5, 1 << 20] {
        let mut src = TestSource::new(bytes.clone());
        let mut sink = VecSink::default();
        let written = write_lossy(&mut src, 0, chunk, &mut sink).expect("written");
        assert_eq!(
            String::from_utf8(sink.0).expect("UTF-8"),
            expected,
            "chunk {chunk}"
        );
        assert_eq!(written, expected.len() as u64);
    }
    let mut src = TestSource::new(bytes);
    let mut sink = VecSink::default();
    write_lossy(&mut src, 3, 1 << 20, &mut sink).expect("written");
    assert_eq!(sink.0, &expected.as_bytes()[3..]);
}
