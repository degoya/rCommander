use std::{fs, path::Path};

use rc_vfs::{ArchiveLimits, check_sevenz_block, check_sevenz_header};
use sevenz_rust2::{
    Archive, ArchiveEntry, ArchiveWriter, EncoderConfiguration, Password, SourceReader,
    encoder_options::Lzma2Options,
};

use super::super::{
    Kind, ops_error,
    test_support::{files, job, payload, run},
};

/// Five files of different sizes, by name.
fn five() -> Vec<(String, Vec<u8>)> {
    (0..5_u8)
        .map(|at| {
            (
                format!("f{at}.bin"),
                payload(10_000 + usize::from(at) * 777, at),
            )
        })
        .collect()
}

/// A 7z of `content`: one block for all (`solid`) or one per file, LZMA2 at `level`.
fn write_7z(path: &Path, content: &[(String, Vec<u8>)], solid: bool, level: u32) {
    let mut writer = ArchiveWriter::new(fs::File::create(path).expect("create")).expect("a writer");
    writer.set_content_methods(vec![EncoderConfiguration::from(Lzma2Options::from_level(
        level,
    ))]);
    if solid {
        let entries = content
            .iter()
            .map(|(name, _)| ArchiveEntry::new_file(name))
            .collect();
        let readers = content
            .iter()
            .map(|(_, bytes)| SourceReader::new(bytes.as_slice()))
            .collect();
        writer
            .push_archive_entries(entries, readers)
            .expect("a block");
    } else {
        for (name, bytes) in content {
            writer
                .push_archive_entry(ArchiveEntry::new_file(name), Some(bytes.as_slice()))
                .expect("an entry");
        }
    }
    writer.finish().expect("finish");
}

/// Acceptance: not solid, one entry removed - only its block goes, the others are copied raw and
/// nothing is decoded.
#[tokio::test]
async fn removing_from_a_non_solid_7z_copies_the_other_blocks_raw() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join("a.7z");
    let mut content = five();
    write_7z(&old, &content, false, 6);
    let update = job(Kind::SevenZ, &old, &[2], Vec::new());
    let temp = update.temp.clone();
    let stats = run(update).await.expect("the update");
    assert_eq!(stats.raw_blocks.len(), 4);
    assert_eq!((stats.raw, stats.decoded, stats.entries), (4, 0, 4));
    content.remove(2);
    assert_eq!(files(&temp).await, content);
}

/// Acceptance: solid, one entry removed - its block is decoded and encoded again, the other
/// entries read back as they were.
#[tokio::test]
async fn removing_from_a_solid_block_encodes_it_again() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join("a.7z");
    let mut content = five();
    write_7z(&old, &content, true, 6);
    let update = job(Kind::SevenZ, &old, &[2], Vec::new());
    let temp = update.temp.clone();
    let stats = run(update).await.expect("the update");
    assert!(stats.raw_blocks.is_empty());
    assert_eq!((stats.raw, stats.decoded, stats.entries), (0, 4, 4));
    content.remove(2);
    assert_eq!(files(&temp).await, content);
}

/// Adding to a solid 7z leaves its block raw and puts the new file into a block of its own.
#[tokio::test]
async fn adding_to_a_7z_keeps_its_blocks_raw() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join("a.7z");
    let mut content = five();
    write_7z(&old, &content, true, 6);
    let new = dir.path().join("new.txt");
    fs::write(&new, b"added").expect("write");
    let update = job(
        Kind::SevenZ,
        &old,
        &[],
        vec![super::super::test_support::member(&new, "new.txt")],
    );
    let temp = update.temp.clone();
    let stats = run(update).await.expect("the update");
    assert_eq!(stats.raw_blocks, [0]);
    assert_eq!((stats.raw, stats.decoded, stats.entries), (5, 0, 6));
    content.push(("new.txt".to_owned(), b"added".to_vec()));
    content.sort();
    assert_eq!(files(&temp).await, content);
}

/// Acceptance: a block whose decoder needs more memory than `archive.decoder_memory_mib` allows
/// is refused, and the old archive stays as it was.
#[tokio::test]
async fn a_block_over_the_decoder_memory_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join("a.7z");
    write_7z(&old, &five(), true, 9);
    let before = fs::read(&old).expect("read");
    // The limit just below what the block needs, which the header still passes.
    let mut reader = fs::File::open(&old).expect("open");
    let archive = Archive::read(&mut reader, &Password::empty()).expect("a 7z");
    let fits = |memory: u64| {
        let limits = ArchiveLimits {
            max_decoder_memory: memory,
            ..ArchiveLimits::default()
        };
        check_sevenz_block(&archive.blocks[0], &limits, &old).is_ok()
    };
    let (mut low, mut high) = (0_u64, 1_u64 << 33);
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        if fits(middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let limits = ArchiveLimits {
        max_decoder_memory: low,
        ..ArchiveLimits::default()
    };
    let mut header = fs::File::open(&old).expect("open");
    check_sevenz_header(&mut header, &limits, &old).expect("the header fits the limit");
    let mut update = job(Kind::SevenZ, &old, &[1], Vec::new());
    update.limits = limits;
    let error = run(update).await.expect_err("refused");
    assert_eq!(ops_error(error, &old).code(), "archive_decoder_memory");
    assert!(
        fs::read(&old).expect("read") == before,
        "the old archive stays"
    );
}

/// A stored (COPY) 7z, built byte by byte: one solid block of `a.txt` ("aaaaa") and `b.txt`
/// ("bbbbb"), whose header states `b.txt` as `stated` bytes - the block's unpack size follows, its
/// data stays ten bytes. 7-Zip tests `stated` 6 as "Data Error : b.txt".
fn stored_7z(stated: u8) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = flate2::Crc::new();
        crc.update(bytes);
        crc.sum()
    }
    let (a, b) = (b"aaaaa", b"bbbbb");
    let names: Vec<u8> = ["a.txt", "b.txt"]
        .iter()
        .flat_map(|name| name.encode_utf16().chain([0]).flat_map(u16::to_le_bytes))
        .collect();
    let mut header = vec![
        0x01, // kHeader
        0x04, // kMainStreamsInfo
        0x06,
        0x00,
        0x01,
        0x09,
        10,
        0x00, // kPackInfo: at 0, one stream of 10 bytes
        0x07,
        0x0B,
        0x01,
        0x00, // kUnPackInfo, kFolder: one block, not external
        0x01,
        0x01,
        0x00, // one coder, id size 1: COPY
        0x0C,
        5 + stated,
        0x00, // kCodersUnPackSize
        0x08,
        0x0D,
        0x02,
        0x09,
        5, // kSubStreamsInfo: two, the first 5 bytes, the last the rest
        0x0A,
        0x01, // kCRC, all defined
    ];
    header.extend(crc(a).to_le_bytes());
    header.extend(crc(b).to_le_bytes());
    header.extend([0x00, 0x00]); // kEnd of the sub-streams, of the streams
    let name_size = u8::try_from(names.len() + 1).expect("short names");
    header.extend([0x05, 0x02, 0x11, name_size, 0x00]); // kFilesInfo: two, kName, not external
    header.extend(&names);
    header.extend([0x00, 0x00]); // kEnd of the files, of the header
    let mut start = Vec::new();
    start.extend(10_u64.to_le_bytes()); // the next header's offset after the packed data
    start.extend((header.len() as u64).to_le_bytes());
    start.extend(crc(&header).to_le_bytes());
    let mut archive = b"7z\xBC\xAF\x27\x1C\x00\x04".to_vec();
    archive.extend(crc(&start).to_le_bytes());
    archive.extend(start);
    archive.extend(a);
    archive.extend(b);
    archive.extend(header);
    archive
}

/// sevenz-rust2 0.25 (RC-110-11): a kept entry of a block encoded again whose data ends before
/// its stated size fails the update; 0.24 handed it out as complete, unchecked, and the new
/// archive kept it short. The same archive with a true size updates.
#[tokio::test]
async fn a_kept_entry_shorter_than_stated_fails_the_update() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join("a.7z");
    fs::write(&old, stored_7z(5)).expect("write");
    let update = job(Kind::SevenZ, &old, &[0], Vec::new());
    let temp = update.temp.clone();
    run(update).await.expect("the true archive updates");
    assert_eq!(
        files(&temp).await,
        [("b.txt".to_owned(), b"bbbbb".to_vec())]
    );
    fs::remove_file(&temp).expect("remove");

    let before = stored_7z(6);
    fs::write(&old, &before).expect("write");
    let error = run(job(Kind::SevenZ, &old, &[0], Vec::new()))
        .await
        .expect_err("a short entry is no entry");
    assert!(
        error.to_string().contains("shorter than declared size"),
        "{error}"
    );
    assert!(
        fs::read(&old).expect("read") == before,
        "the old archive stays"
    );
}

/// Acceptance: an encrypted 7z - its content or its header - is not changed.
#[test]
fn an_encrypted_7z_is_refused() {
    for name in ["encrypted.7z", "encrypted-header.7z"] {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../rc-vfs/tests/fixtures/archives")
            .join(name);
        let error = super::scan(&fixture, &ArchiveLimits::default()).expect_err(name);
        assert_eq!(
            ops_error(error, &fixture).code(),
            "archive_encrypted",
            "{name}"
        );
    }
}
