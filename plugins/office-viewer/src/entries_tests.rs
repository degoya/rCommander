use super::*;
use crate::test_support::{TestSource, zip_of};

#[test]
fn reads_the_count_and_the_directory_of_a_zip() {
    let zip = zip_of(&[("a.xml", b"<a/>"), ("b.xml", b"<b/>"), ("c.xml", b"<c/>")]);
    let length = zip.len() as u64;
    let mut src = TestSource::new(zip);
    let end = find(&mut src).expect("read").expect("a zip");
    assert_eq!(end.declared, 3);
    assert_eq!(end.entries, 3);
    assert_eq!(end.offset + end.size, length - 22);
    assert_eq!(src.reads, 1);
}

#[test]
fn a_file_without_an_end_record_is_no_zip() {
    let mut src = TestSource::new(b"just some text, no zip at all".to_vec());
    assert_eq!(find(&mut src).expect("read"), None);
    let mut tiny = TestSource::new(b"PK".to_vec());
    assert_eq!(find(&mut tiny).expect("read"), None);
}

#[test]
fn a_decoy_record_in_the_comment_neither_hides_the_count_nor_takes_the_directory() {
    let mut zip = zip_of(&[("a.xml", b"<a/>"), ("b.xml", b"<b/>")]);
    // A second end record declaring 7 entries, appended where the comment would be.
    let length = zip.len();
    zip[length - 2..].copy_from_slice(&22u16.to_le_bytes());
    let mut decoy = vec![0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 7, 0, 7, 0];
    decoy.extend_from_slice(&[0; 10]);
    zip.extend_from_slice(&decoy);
    let mut src = TestSource::new(zip);
    let end = find(&mut src).expect("read").expect("a zip");
    assert_eq!(end.declared, 7);
    assert_eq!(end.entries, 2);
}

#[test]
fn reads_the_zip64_record_when_the_count_overflows() {
    // A Zip64 end record declaring 70 000 entries, its locator and an end record saying 0xFFFF.
    let mut bytes = vec![0u8; 16];
    let record_at = bytes.len() as u64;
    let mut record = vec![0x50, 0x4b, 0x06, 0x06];
    record.extend_from_slice(&44u64.to_le_bytes());
    record.extend_from_slice(&[0; 12]);
    record.extend_from_slice(&70_000u64.to_le_bytes());
    record.extend_from_slice(&70_000u64.to_le_bytes());
    record.extend_from_slice(&5u64.to_le_bytes());
    record.extend_from_slice(&9u64.to_le_bytes());
    assert_eq!(record.len(), 56);
    bytes.extend_from_slice(&record);
    let mut locator = vec![0x50, 0x4b, 0x06, 0x07, 0, 0, 0, 0];
    locator.extend_from_slice(&record_at.to_le_bytes());
    locator.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&locator);
    let mut end = vec![0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF];
    end.extend_from_slice(&[0xFF; 8]);
    end.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(&end);
    let mut src = TestSource::new(bytes);
    let found = find(&mut src).expect("read").expect("a zip");
    assert_eq!(
        found,
        End {
            declared: 70_000,
            entries: 70_000,
            offset: 9,
            size: 5,
        }
    );
}
