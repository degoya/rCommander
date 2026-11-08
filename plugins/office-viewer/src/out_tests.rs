use super::*;
use crate::test_support::VecSink;

#[test]
fn writes_in_windows_and_in_order() {
    let mut sink = VecSink::default();
    let mut out = TextOut::new(&mut sink, u64::MAX);
    let line = "x".repeat(1000) + "\n";
    for _ in 0..3000 {
        out.push(&line).expect("push");
    }
    assert_eq!(out.finish().expect("finish"), 3000 * 1001);
    assert_eq!(sink.0.len(), 3000 * 1001);
    assert!(sink.0.chunks(1001).all(|chunk| chunk == line.as_bytes()));
}

#[test]
fn stops_at_the_cap_on_a_character_boundary() {
    let mut sink = VecSink::default();
    let mut out = TextOut::new(&mut sink, 5);
    out.push("abc").expect("push");
    assert!(!out.full());
    out.push("d\u{e9}f").expect("push");
    assert!(out.full());
    out.push("more").expect("push");
    assert_eq!(out.finish().expect("finish"), 4);
    assert_eq!(sink.0, b"abcd");
}
