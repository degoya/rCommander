use std::io::{self, Read};

use zip::CompressionMethod;

use super::*;
use crate::test_support::{TestSource, zip_of, zip_with};

fn code_of(failure: &Failure) -> Option<&str> {
    failure.code.as_deref()
}

fn read_part(package: &mut Package<TestSource>, name: &str) -> Result<Vec<u8>, Failure> {
    let mut part = package.part(name)?.expect("the part exists");
    let mut bytes = Vec::new();
    part.read_to_end(&mut bytes)
        .map_err(|error| failure_of_io(&error))?;
    Ok(bytes)
}

#[test]
fn capped_inflates_at_most_one_byte_past_the_cap() {
    let limits = Limits {
        max_part_bytes: 1000,
        ratio_floor: u64::MAX,
        ..Limits::DEFAULT
    };
    // An endless stream, as a lying entry would inflate.
    let mut capped = Capped::new(io::repeat(b'x'), 10, limits);
    let mut bytes = Vec::new();
    let error = capped.read_to_end(&mut bytes).expect_err("too large");
    assert_eq!(code_of(&failure_of_io(&error)), Some(code::TOO_LARGE));
    assert_eq!(capped.inflated, 1001);
}

#[test]
fn capped_stops_a_part_past_the_ratio_once_past_the_floor() {
    let limits = Limits {
        max_ratio: 5,
        ratio_floor: 100,
        ..Limits::DEFAULT
    };
    let mut capped = Capped::new(io::repeat(0).take(10_000), 10, limits);
    let mut bytes = Vec::new();
    let error = capped.read_to_end(&mut bytes).expect_err("too compressed");
    assert_eq!(code_of(&failure_of_io(&error)), Some(code::TOO_COMPRESSED));
    // Under the floor the ratio is not asked.
    let mut small = Capped::new(io::repeat(0).take(100), 1, limits);
    let mut bytes = Vec::new();
    assert_eq!(small.read_to_end(&mut bytes).expect("under the floor"), 100);
}

#[test]
fn parts_are_found_by_name_in_any_case() {
    let zip = zip_of(&[("word/document.xml", b"<w:document/>"), ("a.txt", b"hello")]);
    let mut package = Package::open(TestSource::new(zip), Limits::DEFAULT).expect("open");
    assert!(package.contains("Word/Document.XML"));
    assert!(!package.contains("word/missing.xml"));
    assert_eq!(
        read_part(&mut package, "WORD/document.xml").expect("read"),
        b"<w:document/>"
    );
    assert!(
        package
            .part("word/missing.xml")
            .expect("no failure")
            .is_none()
    );
}

#[test]
fn a_bin_part_is_never_opened() {
    // Not even a valid zip entry's worth of sense: if it were read, it would fail.
    let zip = zip_with(&[
        (
            "word/vbaProject.bin",
            &[0xD0u8, 0xCF, 0x11, 0xE0][..],
            CompressionMethod::Stored,
        ),
        (
            "word/document.xml",
            b"<w:document/>",
            CompressionMethod::Deflated,
        ),
    ]);
    let mut package = Package::open(TestSource::new(zip), Limits::DEFAULT).expect("open");
    assert!(package.contains("word/vbaProject.bin"));
    assert!(
        package
            .part("word/vbaProject.bin")
            .expect("no failure")
            .is_none()
    );
    assert!(
        package
            .part("Word/VBAPROJECT.BIN")
            .expect("no failure")
            .is_none()
    );
}

#[test]
fn a_part_declared_past_the_cap_is_refused_before_it_is_read() {
    let limits = Limits {
        max_part_bytes: 64 << 10,
        ..Limits::DEFAULT
    };
    let big = vec![b'a'; 100 << 10];
    let zip = zip_with(&[("big.xml", &big[..], CompressionMethod::Stored)]);
    let mut package = Package::open(TestSource::new(zip), limits).expect("open");
    let failure = package.part("big.xml").err().expect("too large");
    assert_eq!(code_of(&failure), Some(code::TOO_LARGE));
}

#[test]
fn a_zip_bomb_is_refused() {
    let zeros = vec![0u8; 4 << 20];
    let zip = zip_of(&[("bomb.xml", &zeros[..])]);
    assert!(
        zip.len() < (4 << 20) / 500,
        "deflate packs zeros past the ratio"
    );
    let mut package = Package::open(TestSource::new(zip), Limits::DEFAULT).expect("open");
    let failure = package.part("bomb.xml").err().expect("too compressed");
    assert_eq!(code_of(&failure), Some(code::TOO_COMPRESSED));
}

#[test]
fn not_a_zip_is_damaged() {
    let failure = Package::open(TestSource::new(b"plain text".to_vec()), Limits::DEFAULT)
        .err()
        .expect("no zip");
    assert_eq!(code_of(&failure), Some(code::DAMAGED));
}

#[test]
fn too_many_entries_are_refused_on_the_end_record() {
    let limits = Limits {
        max_entries: 3,
        ..Limits::DEFAULT
    };
    let zip = zip_of(&[("1", b"1"), ("2", b"2"), ("3", b"3"), ("4", b"4")]);
    let mut src = TestSource::new(zip);
    let failure = Package::open(&mut src, limits).err().expect("too many");
    assert_eq!(code_of(&failure), Some(code::TOO_MANY_ENTRIES));
    assert_eq!(src.reads, 1, "only the end record was read");
}

/// The offset of the first `signature` in `bytes`.
fn position(bytes: &[u8], signature: &[u8]) -> usize {
    bytes
        .windows(signature.len())
        .position(|window| window == signature)
        .expect("the signature is there")
}

fn opened(zip: Vec<u8>) -> Package<TestSource> {
    Package::open(TestSource::new(zip), Limits::DEFAULT).unwrap_or_else(|_| panic!("open"))
}

#[test]
fn stored_and_deflated_parts_read_back() {
    let zip = zip_with(&[
        ("stored.xml", b"<stored/>", CompressionMethod::Stored),
        ("deflated.xml", b"<deflated/>", CompressionMethod::Deflated),
        ("empty.xml", b"", CompressionMethod::Stored),
    ]);
    let mut package = opened(zip);
    assert_eq!(
        read_part(&mut package, "stored.xml").expect("read"),
        b"<stored/>"
    );
    assert_eq!(
        read_part(&mut package, "deflated.xml").expect("read"),
        b"<deflated/>"
    );
    assert_eq!(read_part(&mut package, "empty.xml").expect("read"), b"");
}

#[test]
fn a_part_whose_crc_does_not_match_is_damaged() {
    let mut zip = zip_with(&[("a.xml", b"hello world", CompressionMethod::Stored)]);
    let at = position(&zip, b"hello world");
    zip[at] = b'j';
    let failure = read_part(&mut opened(zip), "a.xml").expect_err("damaged");
    assert_eq!(code_of(&failure), Some(code::DAMAGED));
}

#[test]
fn a_part_inflating_past_its_declared_size_is_stopped() {
    let mut zip = zip_of(&[("a.xml", b"hello hello hello world")]);
    let at = position(&zip, &[0x50, 0x4b, 0x01, 0x02]);
    zip[at + 24..at + 28].copy_from_slice(&4u32.to_le_bytes());
    let failure = read_part(&mut opened(zip), "a.xml").expect_err("damaged");
    assert_eq!(code_of(&failure), Some(code::DAMAGED));
}

#[test]
fn an_encrypted_or_unknown_method_part_is_damaged() {
    let zip = zip_of(&[("a.xml", b"<a/>")]);
    let at = position(&zip, &[0x50, 0x4b, 0x01, 0x02]);
    let mut encrypted = zip.clone();
    encrypted[at + 8] |= 1;
    let failure = opened(encrypted).part("a.xml").err().expect("encrypted");
    assert_eq!(code_of(&failure), Some(code::DAMAGED));
    let mut bzip2 = zip.clone();
    bzip2[at + 10] = 12;
    let failure = opened(bzip2).part("a.xml").err().expect("bzip2");
    assert_eq!(code_of(&failure), Some(code::DAMAGED));
    let mut local = zip;
    local[0] = b'X';
    let failure = opened(local).part("a.xml").err().expect("no local header");
    assert_eq!(code_of(&failure), Some(code::DAMAGED));
}

#[test]
fn a_directory_past_its_bound_is_refused_before_it_is_read() {
    let mut bytes = vec![0u8; 64];
    let mut end = vec![0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 1, 0, 1, 0];
    end.extend_from_slice(&(17u32 << 20).to_le_bytes());
    end.extend_from_slice(&0u32.to_le_bytes());
    end.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(&end);
    let mut src = TestSource::new(bytes);
    let failure = Package::open(&mut src, Limits::DEFAULT)
        .err()
        .expect("too large a directory");
    assert_eq!(code_of(&failure), Some(code::DAMAGED));
    assert_eq!(src.reads, 1);
}
