use super::*;
use crate::MAX_SVG_BYTES;
use crate::test_support::{TestSource, gzip, gzip_zeros};

fn code_of(failure: &Failure) -> Option<&str> {
    failure.code.as_deref()
}

#[test]
fn a_file_is_read_whole_in_windows() {
    let bytes: Vec<u8> = (0..3 * READ_CHUNK + 17).map(|at| at as u8).collect();
    let mut src = TestSource::new(bytes.clone());
    assert_eq!(read_all(&mut src, MAX_SVG_BYTES).expect("read"), bytes);
    assert_eq!(src.reads, 4);
    assert!(src.largest_read <= READ_CHUNK);
}

#[test]
fn a_file_over_the_limit_is_not_read() {
    let mut src = TestSource::new(vec![b'<'; 101]);
    let failure = read_all(&mut src, 100).expect_err("too large");
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(code_of(&failure), Some(code::TOO_LARGE));
    assert_eq!(src.reads, 0);
}

#[test]
fn moving_on_cancels_the_read() {
    let mut src = TestSource::new(vec![b' '; 2 * READ_CHUNK as usize]);
    src.stop_after = Some(1);
    let failure = read_all(&mut src, MAX_SVG_BYTES).expect_err("cancelled");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

#[test]
fn gzip_is_told_by_its_magic() {
    assert!(is_gzip(&gzip(b"<svg/>")));
    assert!(!is_gzip(b"<svg/>"));
    assert!(!is_gzip(b"\x1f"));
}

#[test]
fn svgz_inflates_up_to_the_limit_inclusive() {
    let svg = vec![b'a'; 100];
    let packed = gzip(&svg);
    assert_eq!(inflate(&packed, 100).expect("at the limit"), svg);
    let failure = inflate(&packed, 99).expect_err("over the limit");
    assert_eq!(code_of(&failure), Some(code::TOO_LARGE));
}

#[test]
fn svgz_over_64_mib_inflated_is_refused() {
    // 64 MiB and one byte of zeros, a few hundred kilobytes packed.
    let packed = gzip_zeros(MAX_SVG_BYTES + 1);
    assert!(packed.len() < 1 << 20);
    let failure = inflate(&packed, MAX_SVG_BYTES).expect_err("a bomb");
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(code_of(&failure), Some(code::TOO_LARGE));
}

#[test]
fn a_broken_gzip_stream_is_bad_gzip() {
    let mut packed = gzip(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    let middle = packed.len() / 2;
    packed.truncate(middle);
    let failure = inflate(&packed, MAX_SVG_BYTES).expect_err("cut");
    assert_eq!(code_of(&failure), Some(code::BAD_GZIP));
}

#[test]
fn memory_serves_windows_and_stops_at_its_end() {
    let mut memory = Memory::new(Rc::new(b"0123456789".to_vec()));
    assert_eq!(memory.size(), 10);
    assert_eq!(memory.read_at(2, 3).expect("read"), b"234");
    assert_eq!(memory.read_at(8, 5).expect("read"), b"89");
    assert!(memory.read_at(20, 5).expect("read").is_empty());
}
