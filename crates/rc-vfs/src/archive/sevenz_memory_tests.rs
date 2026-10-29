use std::path::Path;

use sevenz_rust2::{Archive, EncoderMethod, Password};

use super::{check_block, check_header, coder_memory, encoded_header_coders};
use crate::Vfs;
use crate::archive::{
    ArchiveFormat, ArchiveFs, ArchiveLimits, ArchiveSettings,
    test_support::{code, fixture, local, sevenz_encoded_header, temp_file},
};

fn within(max_decoder_memory: u64) -> ArchiveSettings {
    ArchiveSettings {
        limits: ArchiveLimits {
            max_decoder_memory,
            ..ArchiveLimits::default()
        },
        ..ArchiveSettings::default()
    }
}

#[test]
fn each_method_states_its_memory_in_its_properties() {
    // LZMA2: 2 or 3 shifted by the bits' half; 40 is the largest, 4 GiB less one byte.
    assert_eq!(coder_memory(EncoderMethod::ID_LZMA2, &[24]), 16 << 20);
    assert_eq!(coder_memory(EncoderMethod::ID_LZMA2, &[25]), 24 << 20);
    assert_eq!(coder_memory(EncoderMethod::ID_LZMA2, &[40]), 0xFFFF_FFFF);
    // LZMA and PPMd: a little-endian size after the first byte.
    let stated = [0x5D, 0x00, 0x00, 0x00, 0x04];
    assert_eq!(coder_memory(EncoderMethod::ID_LZMA, &stated), 64 << 20);
    assert_eq!(coder_memory(EncoderMethod::ID_PPMD, &stated), 64 << 20);
    // Short properties and other methods ask for nothing here.
    assert_eq!(coder_memory(EncoderMethod::ID_LZMA, &[0x5D]), 0);
    assert_eq!(coder_memory(EncoderMethod::ID_BZIP2, &[]), 0);
}

#[test]
fn the_coders_of_an_encoded_header_are_read_from_its_bytes() {
    let bytes = sevenz_encoded_header(EncoderMethod::ID_LZMA2, &[40], false);
    let coders = encoded_header_coders(&bytes[48..]).expect("the header reads");
    assert_eq!(coders, vec![(EncoderMethod::ID_LZMA2.to_vec(), vec![40])]);
    assert_eq!(encoded_header_coders(&bytes[48..60]), None, "cut short");
    assert_eq!(encoded_header_coders(&[0x01]), None, "a plain header");
}

/// A 7z of a hundred bytes whose encoded header asks for a 4 GiB dictionary: refused before
/// sevenz-rust2 decodes the header, with the start header filled in or blank.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_encoded_header_over_the_limit_is_refused_before_it_is_decoded() {
    for blank in [false, true] {
        let bytes = sevenz_encoded_header(EncoderMethod::ID_LZMA2, &[40], blank);
        let (_dir, path) = temp_file("bomb.7z", &bytes);
        let fs = ArchiveFs::new(local(), path, ArchiveFormat::SevenZ, within(256 << 20));
        assert_eq!(
            code(fs.list(Path::new("/")).await),
            "archive_decoder_memory",
            "blank start header: {blank}"
        );
    }
    // An LZMA header states its dictionary too.
    let lzma = [0x5D, 0x00, 0x00, 0x00, 0x40];
    let bytes = sevenz_encoded_header(EncoderMethod::ID_LZMA, &lzma, false);
    let (_dir, path) = temp_file("lzma.7z", &bytes);
    let fs = ArchiveFs::new(local(), path, ArchiveFormat::SevenZ, within(256 << 20));
    assert_eq!(
        code(fs.list(Path::new("/")).await),
        "archive_decoder_memory"
    );
}

/// Under the limit the check lets the header through; its nonsense bytes are then damaged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_encoded_header_within_the_limit_goes_on_to_sevenz() {
    // 4 MiB of dictionary.
    let bytes = sevenz_encoded_header(EncoderMethod::ID_LZMA2, &[20], false);
    let (_dir, path) = temp_file("small.7z", &bytes);
    let refused = ArchiveFs::new(local(), &path, ArchiveFormat::SevenZ, within(1 << 20));
    assert_eq!(
        code(refused.list(Path::new("/")).await),
        "archive_decoder_memory"
    );
    let passed = ArchiveFs::new(local(), &path, ArchiveFormat::SevenZ, within(8 << 20));
    let listed = code(passed.list(Path::new("/")).await);
    assert_ne!(listed, "archive_decoder_memory");
    assert_ne!(listed, "ok");
}

/// A content block is checked before it is read: `tree.7z` (py7zr) asks for 16 MiB.
#[test]
fn a_block_is_checked_against_the_limit() {
    let mut file = std::fs::File::open(fixture("tree.7z")).expect("tree.7z");
    let archive = Archive::read(&mut file, &Password::empty()).expect("tree.7z reads");
    let block = archive.blocks.first().expect("a block");
    let path = Path::new("tree.7z");
    let refused = check_block(block, &within(8 << 20).limits, path);
    assert_eq!(code(refused), "archive_decoder_memory");
    assert_eq!(
        code(check_block(block, &within(16 << 20).limits, path)),
        "ok"
    );
    // The fixtures open within the default limit, encoded header and all.
    let mut file = std::fs::File::open(fixture("tree.7z")).expect("tree.7z");
    let header = check_header(&mut file, &ArchiveLimits::default(), path);
    assert_eq!(code(header), "ok");
    let mut file = std::fs::File::open(fixture("tree.7z")).expect("tree.7z");
    assert_eq!(
        code(check_header(&mut file, &within(8 << 20).limits, path)),
        "archive_decoder_memory"
    );
}
