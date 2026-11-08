use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};

use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::super::{
    Kind, Refused, refusal,
    test_support::{files, job, member, payload, run},
};

/// What a zip stores of one entry: its compressed bytes, CRC, size and packed size.
type Stored = (Vec<u8>, u32, u64, u64);

/// A zip of `count` deflated files `dir/file-NNNN.txt` and a comment.
fn write_zip(path: &Path, count: usize) {
    let mut zip = ZipWriter::new(fs::File::create(path).expect("create"));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for at in 0..count {
        zip.start_file(format!("dir/file-{at:04}.txt"), options)
            .expect("start");
        zip.write_all(&payload(300 + at % 97, at as u8))
            .expect("write");
    }
    zip.set_comment("kept comment").expect("comment");
    zip.finish().expect("finish");
}

/// Every entry as stored, by name.
fn stored(path: &Path) -> BTreeMap<String, Stored> {
    let mut archive = ZipArchive::new(fs::File::open(path).expect("open")).expect("a zip");
    let mut all = BTreeMap::new();
    for index in 0..archive.len() {
        let mut member = archive.by_index_raw(index).expect("an entry");
        let mut bytes = Vec::new();
        member.read_to_end(&mut bytes).expect("its stored bytes");
        let name = member.name().expect("a UTF-8 name").into_owned();
        all.insert(
            name,
            (
                bytes,
                member.crc32(),
                member.size(),
                member.compressed_size(),
            ),
        );
    }
    all
}

/// Acceptance: 1 MiB into a zip of 1 000 entries - every old entry keeps its compressed bytes,
/// CRC and sizes, and none is inflated; the archive's comment stays.
#[tokio::test]
async fn adding_copies_every_old_entry_raw() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join("a.zip");
    write_zip(&old, 1000);
    let big = dir.path().join("big.bin");
    let bytes = payload(1 << 20, 9);
    fs::write(&big, &bytes).expect("write");
    let before = stored(&old);
    let update = job(Kind::Zip, &old, &[], vec![member(&big, "sub/big.bin")]);
    let temp = update.temp.clone();
    let stats = run(update).await.expect("the update");
    assert_eq!((stats.raw, stats.decoded, stats.entries), (1000, 0, 1001));
    let after = stored(&temp);
    for (name, entry) in &before {
        assert_eq!(after.get(name), Some(entry), "{name}");
    }
    let mut archive = ZipArchive::new(fs::File::open(&temp).expect("open")).expect("a zip");
    assert_eq!(archive.comment(), b"kept comment");
    let mut added = Vec::new();
    archive
        .by_name("sub/big.bin")
        .expect("the new entry")
        .read_to_end(&mut added)
        .expect("read");
    assert!(added == bytes, "the new entry reads back");
}

/// Acceptance: one entry removed, every other one byte for byte as it was.
#[tokio::test]
async fn removing_keeps_the_others_byte_for_byte() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join("a.zip");
    write_zip(&old, 20);
    let mut before = stored(&old);
    let update = job(Kind::Zip, &old, &[7], Vec::new());
    let temp = update.temp.clone();
    let stats = run(update).await.expect("the update");
    assert_eq!((stats.raw, stats.decoded, stats.entries), (19, 0, 19));
    assert!(before.remove("dir/file-0007.txt").is_some());
    assert_eq!(stored(&temp), before);
}

/// A cleaned name stands only for its own index: removing `../evil.txt` (listed as `evil.txt`)
/// keeps the entry really named `evil.txt`.
#[tokio::test]
async fn removing_by_index_never_hits_a_namesake() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join("a.zip");
    let mut zip = ZipWriter::new(fs::File::create(&old).expect("create"));
    for (name, text) in [("../evil.txt", b"climbs"), ("evil.txt", b"stays!")] {
        zip.start_file(name, SimpleFileOptions::default())
            .expect("start");
        zip.write_all(text).expect("write");
    }
    zip.finish().expect("finish");
    let update = job(Kind::Zip, &old, &[0], Vec::new());
    let temp = update.temp.clone();
    run(update).await.expect("the update");
    assert_eq!(
        files(&temp).await,
        [("evil.txt".to_owned(), b"stays!".to_vec())]
    );
}

/// A zip behind a prefix (a self-extractor) is not changed.
#[tokio::test]
async fn a_zip_behind_a_prefix_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let plain = dir.path().join("plain.zip");
    write_zip(&plain, 3);
    let old = dir.path().join("setup.exe");
    let mut bytes = b"MZ".to_vec();
    bytes.resize(4096, 0);
    bytes.extend(fs::read(&plain).expect("read"));
    fs::write(&old, bytes).expect("write");
    let error = run(job(Kind::Zip, &old, &[0], Vec::new()))
        .await
        .expect_err("refused");
    assert_eq!(refusal(&error), Some(Refused::NotModifiable));
}

/// Acceptance: an encrypted zip is not changed - no password is asked to write.
#[tokio::test]
async fn an_encrypted_zip_is_refused() {
    for name in ["encrypted-zipcrypto.zip", "encrypted-aes.zip"] {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../rc-vfs/tests/fixtures/archives")
            .join(name);
        let error = super::scan(&fixture).expect_err(name);
        assert_eq!(refusal(&error), Some(Refused::Encrypted), "{name}");
    }
}
