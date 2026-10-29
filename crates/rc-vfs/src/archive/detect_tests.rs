use std::{fs::File, io};

use super::{is_tar, sniff};
use crate::archive::{
    ArchiveFormat, ArchiveFs, ArchiveLimits, ArchiveSettings, Compression,
    test_support::{fixture, local},
};

#[test]
fn every_fixture_is_told_by_its_bytes() {
    let tar = |compression| Some(ArchiveFormat::Tar { compression });
    let single = |compression| Some(ArchiveFormat::Single { compression });
    let cases = [
        ("tree.zip", Some(ArchiveFormat::Zip)),
        ("zip64.zip", Some(ArchiveFormat::Zip)),
        ("encrypted-aes.zip", Some(ArchiveFormat::Zip)),
        ("tree.7z", Some(ArchiveFormat::SevenZ)),
        ("encrypted-header.7z", Some(ArchiveFormat::SevenZ)),
        ("tree.tar", tar(None)),
        ("unsafe.tar", tar(None)),
        ("tree.tar.gz", tar(Some(Compression::Gzip))),
        ("bomb.tar.gz", tar(Some(Compression::Gzip))),
        ("tree.tar.xz", tar(Some(Compression::Xz))),
        ("tree.tar.bz2", tar(Some(Compression::Bzip2))),
        ("tree.tar.zst", tar(Some(Compression::Zstd))),
        ("single.txt.gz", single(Compression::Gzip)),
        ("single.txt.xz", single(Compression::Xz)),
        ("single.txt.bz2", single(Compression::Bzip2)),
        ("single.txt.zst", single(Compression::Zstd)),
        ("frames.txt.zst", single(Compression::Zstd)),
        ("not-a-zip.zip", None),
        ("generate.py", None),
    ];
    for (name, want) in cases {
        let file = File::open(fixture(name)).expect(name);
        let format = sniff(file, &ArchiveLimits::default()).expect(name);
        assert_eq!(format, want, "{name}");
    }
}

#[test]
fn an_empty_file_is_no_archive() {
    let format = sniff(io::empty(), &ArchiveLimits::default()).expect("empty");
    assert_eq!(format, None);
}

/// An old tar has no `ustar` magic; its checksum tells it.
#[test]
fn a_v7_tar_is_told_by_its_checksum() {
    let mut block = [0_u8; 512];
    block[..5].copy_from_slice(b"a.txt");
    block[100..108].copy_from_slice(b"0000644\0");
    block[124..136].copy_from_slice(b"00000000005\0");
    block[156] = b'0';
    // The checksum field counts as eight spaces; it is all zeros now.
    let sum: u32 = block.iter().map(|&byte| u32::from(byte)).sum::<u32>() + 8 * u32::from(b' ');
    block[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    assert!(is_tar(&block));
    block[0] = b'b';
    assert!(!is_tar(&block), "a changed byte breaks the checksum");
    assert!(!is_tar(&[0; 512]), "an empty block is no header");
    assert!(!is_tar(&block[..100]), "a short block is no header");
}

/// Acceptance: a file named `.zip` that is none is not opened as an archive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_named_zip_that_is_none_is_not_opened() {
    let opened = ArchiveFs::open(
        local(),
        fixture("not-a-zip.zip"),
        ArchiveSettings::default(),
    )
    .await
    .expect("readable");
    assert!(opened.is_none());
}
