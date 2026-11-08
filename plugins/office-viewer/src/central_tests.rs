use super::*;
use crate::entries::find;
use crate::test_support::{TestSource, zip_of};

/// The central directory of `zip` and its entry count.
fn directory(zip: &[u8]) -> (Vec<u8>, u64) {
    let end = find(&mut TestSource::new(zip.to_vec()))
        .expect("read")
        .expect("a zip");
    let start = end.offset as usize;
    (zip[start..start + end.size as usize].to_vec(), end.entries)
}

/// One directory header with `extra` as its extra field.
fn header(name: &str, packed: u32, size: u32, offset: u32, extra: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0, 8, 0, 0, 0, 0, 0];
    bytes.extend_from_slice(&0x1234_5678u32.to_le_bytes());
    bytes.extend_from_slice(&packed.to_le_bytes());
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    // Comment length, disk number, internal and external attributes.
    bytes.extend_from_slice(&[0; 10]);
    bytes.extend_from_slice(&offset.to_le_bytes());
    assert_eq!(bytes.len(), 46);
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(extra);
    bytes
}

#[test]
fn reads_the_entries_zip_writes() {
    let zip = zip_of(&[("word/document.xml", b"<w:document/>"), ("a.txt", b"hello")]);
    let (bytes, count) = directory(&zip);
    let entries = parse(&bytes, count).expect("parse");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "word/document.xml");
    assert_eq!(entries[0].method, DEFLATED);
    assert_eq!(entries[0].size, 13);
    assert_eq!(entries[0].header, 0);
    assert!(!entries[0].encrypted());
    assert_eq!(entries[1].name, "a.txt");
    assert_eq!(entries[1].size, 5);
}

#[test]
fn the_zip64_field_replaces_the_saturated_ones_in_order() {
    let mut extra = vec![0x99, 0x99, 2, 0, 0, 0];
    extra.extend_from_slice(&[0x01, 0x00, 16, 0]);
    extra.extend_from_slice(&(5u64 << 32).to_le_bytes());
    extra.extend_from_slice(&(7u64 << 32).to_le_bytes());
    let bytes = header("big.xml", 100, SATURATED, SATURATED, &extra);
    let entries = parse(&bytes, 1).expect("parse");
    assert_eq!(entries[0].size, 5 << 32);
    assert_eq!(entries[0].packed, 100);
    assert_eq!(entries[0].header, 7 << 32);
    assert_eq!(entries[0].crc, 0x1234_5678);
}

#[test]
fn a_saturated_field_without_its_zip64_value_is_damaged() {
    let bytes = header("big.xml", SATURATED, 1, 0, &[0x01, 0x00, 0, 0]);
    assert_eq!(
        parse(&bytes, 1).err().and_then(|failure| failure.code),
        Some(crate::limits::code::DAMAGED.to_owned())
    );
}

#[test]
fn a_short_or_foreign_directory_is_damaged() {
    let bytes = header("a.xml", 1, 1, 0, &[]);
    assert!(
        parse(&bytes, 2).is_err(),
        "one header where two are declared"
    );
    assert!(parse(&bytes[..40], 1).is_err());
    let mut foreign = bytes.clone();
    foreign[2] = 0x03;
    assert!(parse(&foreign, 1).is_err());
    let mut long_name = bytes;
    long_name[28] = 200;
    assert!(parse(&long_name, 1).is_err());
}
