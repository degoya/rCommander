use std::io::{BufRead, Read, Seek, SeekFrom};

use super::*;
use crate::test_support::named;

fn bytes(count: usize) -> Vec<u8> {
    (0..count).map(|index| (index % 251) as u8).collect()
}

#[test]
fn reads_and_seeks_like_a_file() {
    let data = bytes(10_000);
    let mut src = named(&data, "x");
    let mut reader = SourceReader::new(&mut src, 64, u64::MAX);
    let mut first = [0u8; 100];
    reader.read_exact(&mut first).expect("read");
    assert_eq!(&first[..], &data[..100]);
    assert_eq!(reader.seek(SeekFrom::End(-10)).expect("seek"), 9_990);
    let mut rest = Vec::new();
    reader.read_to_end(&mut rest).expect("read to the end");
    assert_eq!(rest, &data[9_990..]);
    assert_eq!(
        reader.seek(SeekFrom::Current(-4)).expect("seek back"),
        9_996
    );
    assert!(reader.seek(SeekFrom::Current(-20_000)).is_err());
    // Past the end reads nothing.
    reader
        .seek(SeekFrom::Start(20_000))
        .expect("seek past the end");
    assert_eq!(reader.read(&mut first).expect("read"), 0);
    reader.seek(SeekFrom::Start(5)).expect("seek");
    assert_eq!(reader.fill_buf().expect("fill").first(), Some(&data[5]));
    reader.consume(3);
    assert_eq!(reader.stream_position().expect("position"), 8);
}

#[test]
fn exact_reads_across_windows_and_refuses_a_short_file() {
    let data = bytes(3 << 20);
    let mut src = named(&data, "x");
    let mut reader = SourceReader::new(&mut src, 4096, u64::MAX);
    let middle = reader.exact(1000, 2 << 20, "short").expect("exact");
    assert_eq!(middle, &data[1000..1000 + (2 << 20)]);
    let failure = reader.exact(3 << 20, 1, "short").expect_err("past the end");
    assert_eq!(failure.code.as_deref(), Some("short"));
    let failure = reader.exact(u64::MAX, 2, "short").expect_err("overflow");
    assert_eq!(failure.kind, FailureKind::BadData);
}

#[test]
fn the_budget_ends_reading() {
    let data = bytes(100_000);
    let mut src = named(&data, "x");
    let mut reader = SourceReader::new(&mut src, 1024, 4096);
    let mut sink = Vec::new();
    let error = reader.read_to_end(&mut sink).expect_err("over budget");
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    let failure = reader.take_failure().expect("the failure is kept");
    assert_eq!(failure.code.as_deref(), Some(codes::TOO_LARGE));
    assert_eq!(sink.len(), 4096);
}

#[test]
fn a_cancel_is_kept_for_the_caller() {
    let data = bytes(10_000);
    let mut src = named(&data, "x");
    src.stop_after = Some(0);
    let mut reader = SourceReader::new(&mut src, 1024, u64::MAX);
    let mut buffer = [0u8; 10];
    assert!(reader.read(&mut buffer).is_err());
    let failure = reader.take_failure().expect("the failure is kept");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

#[test]
fn big_endian_numbers_read_only_when_whole() {
    let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
    assert_eq!(be_u16(&data, 0), Some(0x1234));
    assert_eq!(be_u32(&data, 4), Some(0x9ABC_DEF0));
    assert_eq!(be_u64(&data, 0), Some(0x1234_5678_9ABC_DEF0));
    assert_eq!(be_u32(&data, 6), None);
    assert_eq!(be_u16(&data, usize::MAX), None);
    assert_eq!(be_uint(&[1, 0]), 256);
}
