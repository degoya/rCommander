use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

use super::{decoder, guarded};
use crate::{
    VfsError,
    archive::{
        ArchiveFormat, ArchiveFs, ArchiveLimits, ArchiveSettings, Compression,
        test_support::{code, fixture, local, temp_file, xz_dictionary_bomb, zstd_window_bomb},
    },
};

fn payload() -> Vec<u8> {
    b"single file payload\n".repeat(100)
}

#[test]
fn every_compression_unpacks_its_fixture() {
    let cases = [
        ("single.txt.gz", Compression::Gzip),
        ("single.txt.xz", Compression::Xz),
        ("single.txt.bz2", Compression::Bzip2),
        ("single.txt.zst", Compression::Zstd),
        // Two zstd frames: the decoder must not stop after the first.
        ("frames.txt.zst", Compression::Zstd),
    ];
    for (name, compression) in cases {
        let file = File::open(fixture(name)).expect(name);
        let mut unpacked = Vec::new();
        decoder(compression, file, &ArchiveLimits::default())
            .expect(name)
            .read_to_end(&mut unpacked)
            .expect(name);
        assert_eq!(unpacked, payload(), "{name}");
    }
}

/// Reads until the guard refuses; the bytes it let through, and its error.
fn until_refused(limits: ArchiveLimits) -> (u64, VfsError) {
    let file = File::open(fixture("bomb.tar.gz")).expect("bomb.tar.gz");
    let mut guard =
        guarded(Compression::Gzip, file, limits, Path::new("bomb.tar.gz")).expect("guard");
    let mut through = 0_u64;
    let mut buffer = vec![0; 8192];
    loop {
        match guard.read(&mut buffer) {
            Ok(0) => panic!("the bomb unpacked whole, {through} bytes"),
            Ok(read) => through += read as u64,
            Err(error) => return (through, VfsError::from_io(error, "bomb.tar.gz")),
        }
    }
}

/// 16 MiB of zeros in 16 KiB of gzip: refused once past the threshold, not at the end.
#[test]
fn the_guard_stops_a_stream_over_the_ratio() {
    let (through, error) = until_refused(ArchiveLimits {
        max_ratio: 100,
        ratio_threshold: 1 << 20,
        ..ArchiveLimits::default()
    });
    assert!(
        matches!(
            error,
            VfsError::ArchiveLimit {
                limit: "compression ratio",
                ..
            }
        ),
        "{error:?}"
    );
    assert!(through < 4 << 20, "{through} bytes went through");
}

#[test]
fn the_guard_stops_a_stream_over_the_unpacked_size() {
    let (through, error) = until_refused(ArchiveLimits {
        max_unpacked_size: 1 << 20,
        ..ArchiveLimits::default()
    });
    assert!(
        matches!(
            error,
            VfsError::ArchiveLimit {
                limit: "unpacked size",
                ..
            }
        ),
        "{error:?}"
    );
    assert!(through <= 1 << 20, "{through} bytes went through");
}

#[test]
fn within_the_limits_the_guard_is_invisible() {
    let file = File::open(fixture("bomb.tar.gz")).expect("bomb.tar.gz");
    let mut guard = guarded(
        Compression::Gzip,
        file,
        ArchiveLimits::default(),
        Path::new("bomb.tar.gz"),
    )
    .expect("guard");
    let unpacked = io::copy(&mut guard, &mut io::sink()).expect("unpack");
    assert!(unpacked > 16 << 20, "{unpacked}");
}

/// RC-080-13: a zstd frame that asks for a 2 GiB window is refused when its header is read,
/// before any window is set aside - an error at once, also as the second frame of a stream.
#[test]
fn a_zstd_frame_asking_for_a_huge_window_is_refused_at_once() {
    let started = std::time::Instant::now();
    let limits = ArchiveLimits::default();
    let Err(refused) = decoder(
        Compression::Zstd,
        io::Cursor::new(zstd_window_bomb()),
        &limits,
    ) else {
        panic!("the frame is refused");
    };
    assert_eq!(refused.kind(), io::ErrorKind::InvalidData);

    let mut stream = std::fs::read(fixture("single.txt.zst")).expect("fixture");
    stream.extend(zstd_window_bomb());
    let mut unpacked = Vec::new();
    let refused = decoder(Compression::Zstd, io::Cursor::new(stream), &limits)
        .expect("the first frame opens")
        .read_to_end(&mut unpacked)
        .expect_err("the second frame is refused");
    assert_eq!(refused.kind(), io::ErrorKind::InvalidData);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "no hang"
    );
}

fn decoder_memory(max_decoder_memory: u64) -> ArchiveLimits {
    ArchiveLimits {
        max_decoder_memory,
        ..ArchiveLimits::default()
    }
}

/// RC-080-35: an xz block that asks for a 4 GiB dictionary is refused when its header is read,
/// before lzma-rust2 sets anything aside, as `archive_decoder_memory`.
#[test]
fn an_xz_block_over_the_decoder_memory_is_refused_at_once() {
    let bomb = io::Cursor::new(xz_dictionary_bomb(40));
    let mut guard = guarded(
        Compression::Xz,
        bomb,
        ArchiveLimits::default(),
        Path::new("bomb.xz"),
    )
    .expect("guard");
    let error = VfsError::from_io(
        guard.read(&mut [0; 64]).expect_err("the block is refused"),
        "other",
    );
    assert!(
        matches!(&error, VfsError::ArchiveDecoderMemory { path } if path == Path::new("bomb.xz")),
        "{error:?}"
    );
    // The fixture's 8 MiB dictionary is within the default and over a 1 MiB limit.
    let file = File::open(fixture("single.txt.xz")).expect("fixture");
    let mut unpacked = Vec::new();
    let refused = decoder(Compression::Xz, file, &decoder_memory(1 << 20))
        .expect("the stream opens")
        .read_to_end(&mut unpacked)
        .expect_err("the block is refused");
    assert_eq!(refused.kind(), io::ErrorKind::OutOfMemory);
}

/// The limit holds for a tar.xz and a single .xz, and for the detection that looks into both.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tar_xz_and_a_single_xz_over_the_limit_are_refused() {
    let (_dir, path) = temp_file("bomb.tar.xz", &xz_dictionary_bomb(40));
    let settings = ArchiveSettings::default();
    let opened = ArchiveFs::open(local(), &path, settings.clone()).await;
    assert_eq!(code(opened.map(|_| ())), "archive_decoder_memory");
    let formats = [
        ArchiveFormat::Tar {
            compression: Some(Compression::Xz),
        },
        ArchiveFormat::Single {
            compression: Compression::Xz,
        },
    ];
    for format in formats {
        let fs = ArchiveFs::new(local(), &path, format, settings.clone());
        assert_eq!(
            code(crate::Vfs::list(&fs, Path::new("/")).await),
            "archive_decoder_memory",
            "{format:?}"
        );
    }
}
