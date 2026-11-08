use std::io::{Read, Seek, SeekFrom};

use super::*;
use crate::test_support::TestSource;

fn bytes(count: usize) -> Vec<u8> {
    (0..count).map(|at| (at % 251) as u8).collect()
}

#[test]
fn reads_and_seeks_across_windows() {
    let content = bytes(WINDOW as usize * 2 + 10);
    let mut io = SourceIo::new(TestSource::new(content.clone()));
    let mut all = Vec::new();
    io.read_to_end(&mut all).expect("read");
    assert_eq!(all, content);

    let at = io.seek(SeekFrom::End(-5)).expect("seek");
    assert_eq!(at, content.len() as u64 - 5);
    let mut tail = Vec::new();
    io.read_to_end(&mut tail).expect("read the tail");
    assert_eq!(tail, content[content.len() - 5..]);

    io.seek(SeekFrom::Start(WINDOW as u64 - 2)).expect("seek");
    let mut four = [0u8; 4];
    io.read_exact(&mut four).expect("read across a window edge");
    assert_eq!(four[..], content[WINDOW as usize - 2..WINDOW as usize + 2]);
    assert!(io.seek(SeekFrom::Current(-(1 << 40))).is_err());
}

#[test]
fn a_window_is_read_once() {
    let mut src = TestSource::new(bytes(1000));
    {
        let mut io = SourceIo::new(&mut src);
        let mut byte = [0u8; 1];
        for _ in 0..100 {
            io.read_exact(&mut byte).expect("read");
        }
    }
    assert_eq!(src.reads, 1);
}

#[test]
fn should_stop_ends_the_read_as_cancelled() {
    let mut src = TestSource::new(bytes(100));
    src.stop_after = Some(0);
    let mut io = SourceIo::new(src);
    let mut byte = [0u8; 1];
    let error = io.read(&mut byte).expect_err("cancelled");
    assert_eq!(failure_of_io(&error).kind, FailureKind::Cancelled);
}

#[test]
fn an_unknown_io_error_is_a_damaged_package() {
    let failure = failure_of_io(&std::io::Error::other("bad"));
    assert_eq!(failure.code.as_deref(), Some(code::DAMAGED));
    let carried = failure_of_io(&carry(Failure::coded(
        FailureKind::BadData,
        code::TOO_LARGE,
    )));
    assert_eq!(carried.code.as_deref(), Some(code::TOO_LARGE));
}
