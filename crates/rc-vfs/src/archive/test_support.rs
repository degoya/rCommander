//! The fixtures of the archive tests (`tests/fixtures/archives/`, written by its
//! `generate.py`) and the helpers that open and read them.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use tokio::io::AsyncReadExt;

use super::{ArchiveFs, ArchiveSettings};
use crate::{Entry, LocalFs, Result, Vfs};

/// Every timestamp of the fixtures: 2024-05-17 12:34:56 UTC.
pub(crate) fn mtime() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_715_949_296)
}

pub(crate) fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("archives")
        .join(name)
}

pub(crate) fn local() -> Arc<dyn Vfs> {
    Arc::new(LocalFs::new())
}

/// The fixture `name` as an archive, its format detected.
pub(crate) async fn open(name: &str) -> ArchiveFs {
    open_with(name, ArchiveSettings::default()).await
}

pub(crate) async fn open_with(name: &str, settings: ArchiveSettings) -> ArchiveFs {
    open_path(fixture(name), settings).await
}

pub(crate) async fn open_path(path: PathBuf, settings: ArchiveSettings) -> ArchiveFs {
    let shown = path.display().to_string();
    ArchiveFs::open(local(), path, settings)
        .await
        .unwrap_or_else(|error| panic!("open {shown}: {error}"))
        .unwrap_or_else(|| panic!("{shown} is no archive"))
}

/// The whole entry at `path`.
pub(crate) async fn read(fs: &ArchiveFs, path: &str) -> Vec<u8> {
    let mut stream = fs
        .open_read(Path::new(path))
        .await
        .unwrap_or_else(|error| panic!("open {path}: {error}"));
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .await
        .unwrap_or_else(|error| panic!("read {path}: {error}"));
    bytes
}

/// The entries of `dir`, sorted by name.
pub(crate) async fn list(fs: &ArchiveFs, dir: &str) -> Vec<Entry> {
    let mut entries = fs
        .list(Path::new(dir))
        .await
        .unwrap_or_else(|error| panic!("list {dir}: {error}"));
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

pub(crate) fn names(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

/// A zstd frame whose header asks for a 2 GiB window, with one RLE block of 128 KiB: the shape
/// of RUSTSEC-2026-0344's bomb, a few bytes that made `ruzstd` 0.8 set aside its window before a
/// byte came out. Written here byte by byte instead of a committed fixture (RC-080-13).
pub(crate) fn zstd_window_bomb() -> Vec<u8> {
    // Magic, a frame header descriptor with a window descriptor and nothing else, then
    // `Window_Descriptor`: exponent 21 (window log 31, 2 GiB), mantissa 0.
    let mut bytes = vec![0x28, 0xB5, 0x2F, 0xFD, 0x00, 21 << 3];
    // The block header: the last block, of type RLE, regenerating 128 KiB from one byte.
    let block: u32 = 1 | (1 << 1) | ((128 * 1024) << 3);
    bytes.extend_from_slice(&block.to_le_bytes()[..3]);
    bytes.push(0);
    bytes
}

/// The CRC-32 of xz and 7z headers, bit by bit: the fixtures below are a few bytes each.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ 0xEDB8_8320
            };
        }
    }
    !crc
}

/// An xz stream whose first block asks LZMA2 for the dictionary `dictionary_bits` encodes (40:
/// 4 GiB), followed by no data: what RC-080-35 refuses before lzma-rust2 sets it aside.
pub(crate) fn xz_dictionary_bomb(dictionary_bits: u8) -> Vec<u8> {
    // Stream header: magic, flags (CRC32 check), the flags' CRC.
    let mut bytes = b"\xFD7zXZ\x00".to_vec();
    let flags = [0x00, 0x01];
    bytes.extend_from_slice(&flags);
    bytes.extend_from_slice(&crc32(&flags).to_le_bytes());
    // Block header of 12 bytes: one filter (LZMA2, one property byte), padding, its CRC.
    let header = [0x02, 0x00, 0x21, 0x01, dictionary_bits, 0, 0, 0];
    bytes.extend_from_slice(&header);
    bytes.extend_from_slice(&crc32(&header).to_le_bytes());
    bytes.extend_from_slice(&[0; 16]);
    bytes
}

/// A 7z whose encoded header is unpacked with one coder, `method` with `properties`, and whose
/// packed bytes are nonsense. `blank` leaves the start header blank (an unfinished archive), so
/// sevenz-rust2 has to look for the header from the end.
pub(crate) fn sevenz_encoded_header(method: &[u8], properties: &[u8], blank: bool) -> Vec<u8> {
    let mut header = vec![
        0x17, // kEncodedHeader
        0x06, 0x00, 0x01, 0x09, 0x10, 0x00, // kPackInfo: at 0, one stream of 16 bytes
        0x07, 0x0B, 0x01, 0x00, // kUnpackInfo, kFolder: one folder, not external
        0x01, // one coder
    ];
    let flags = u8::try_from(method.len()).expect("a short id") | 0x20;
    header.push(flags);
    header.extend_from_slice(method);
    header.push(u8::try_from(properties.len()).expect("short properties"));
    header.extend_from_slice(properties);
    header.extend_from_slice(&[0x0C, 0x10, 0x00, 0x00]); // kCodersUnpackSize 16, kEnd, kEnd
    let packed = [0x5A_u8; 16];
    let mut start = Vec::new();
    start.extend_from_slice(&(packed.len() as u64).to_le_bytes());
    start.extend_from_slice(&(header.len() as u64).to_le_bytes());
    start.extend_from_slice(&crc32(&header).to_le_bytes());
    let mut bytes = b"7z\xBC\xAF\x27\x1C\x00\x04".to_vec();
    if blank {
        bytes.extend_from_slice(&[0; 24]);
    } else {
        bytes.extend_from_slice(&crc32(&start).to_le_bytes());
        bytes.extend_from_slice(&start);
    }
    bytes.extend_from_slice(&packed);
    bytes.extend_from_slice(&header);
    bytes
}

/// `bytes` as the file `name` in a new temporary directory, kept as long as the directory.
pub(crate) fn temp_file(name: &str, bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).expect("write");
    (dir, path)
}

/// The error code of a result, `ok` for a success.
pub(crate) fn code<T>(result: Result<T>) -> &'static str {
    match result {
        Ok(_) => "ok",
        Err(error) => error.code(),
    }
}
