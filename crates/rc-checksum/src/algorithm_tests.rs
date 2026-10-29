use std::{
    path::Path,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, ReadBuf};

use super::*;
use crate::test_support;

/// The published test vectors: RFC 1321 (MD5), FIPS 180 (SHA), the BLAKE3 reference
/// implementation and the CRC-32 check value.
const VECTORS: &[(Algorithm, &[u8], &str)] = &[
    (Algorithm::Crc32, b"", "00000000"),
    (Algorithm::Crc32, b"123456789", "cbf43926"),
    (Algorithm::Md5, b"", "d41d8cd98f00b204e9800998ecf8427e"),
    (Algorithm::Md5, b"abc", "900150983cd24fb0d6963f7d28e17f72"),
    (
        Algorithm::Sha1,
        b"",
        "da39a3ee5e6b4b0d3255bfef95601890afd80709",
    ),
    (
        Algorithm::Sha1,
        b"abc",
        "a9993e364706816aba3e25717850c26c9cd0d89d",
    ),
    (
        Algorithm::Sha256,
        b"",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    ),
    (
        Algorithm::Sha256,
        b"abc",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    ),
    (
        Algorithm::Sha512,
        b"",
        "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
         47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
    ),
    (
        Algorithm::Sha512,
        b"abc",
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
         2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
    ),
    (
        Algorithm::Blake3,
        b"",
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
    ),
    (
        Algorithm::Blake3,
        b"abc",
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85",
    ),
];

#[test]
fn every_algorithm_matches_its_reference_vectors() {
    for algorithm in Algorithm::ALL {
        assert!(
            VECTORS.iter().any(|(of, _, _)| *of == algorithm),
            "{algorithm:?} has no vector"
        );
    }
    for (algorithm, input, expected) in VECTORS {
        let mut hasher = algorithm.hasher();
        hasher.update(input);
        let hex = hasher.finish_hex();
        assert_eq!(hex, *expected, "{algorithm:?} of {input:?}");
        assert_eq!(hex.len(), algorithm.hex_len(), "{algorithm:?}");
    }
}

#[test]
fn a_digest_does_not_depend_on_how_the_input_is_cut() {
    let data = test_support::pattern(100_000);
    for algorithm in Algorithm::ALL {
        let mut whole = algorithm.hasher();
        whole.update(&data);
        let mut pieces = algorithm.hasher();
        for chunk in data.chunks(777) {
            pieces.update(chunk);
        }
        assert_eq!(whole.finish_hex(), pieces.finish_hex(), "{algorithm:?}");
    }
}

#[test]
fn extensions_and_tags_name_the_algorithm_back() {
    for algorithm in Algorithm::ALL {
        assert_eq!(
            Algorithm::from_extension(algorithm.extension()),
            Some(algorithm)
        );
    }
    assert_eq!(Algorithm::from_extension("SHA1"), Some(Algorithm::Sha1));
    assert_eq!(Algorithm::from_extension("b3"), Some(Algorithm::Blake3));
    assert_eq!(Algorithm::from_extension("txt"), None);
    assert_eq!(Algorithm::from_tag("sha256"), Some(Algorithm::Sha256));
    assert_eq!(Algorithm::from_hex_len(64), Some(Algorithm::Sha256));
    assert_eq!(Algorithm::from_hex_len(7), None);
}

/// RC-020-10: a file name names its algorithm by the extension, whatever its case; a name
/// without one, or with another, names none.
#[test]
fn a_checksum_file_name_names_its_algorithm() {
    let named = [
        ("set.sfv", Algorithm::Crc32),
        ("disc.iso.md5", Algorithm::Md5),
        ("a.sha", Algorithm::Sha1),
        ("a.SHA1", Algorithm::Sha1),
        ("checksums.sha256", Algorithm::Sha256),
        ("a.sha512", Algorithm::Sha512),
        ("a.b3", Algorithm::Blake3),
        ("a.Blake3", Algorithm::Blake3),
    ];
    for (name, algorithm) in named {
        assert_eq!(Algorithm::of_file_name(name), Some(algorithm), "{name}");
    }
    for name in ["sfv", "readme.txt", "set.sfv.txt", "archive.zip", ""] {
        assert_eq!(Algorithm::of_file_name(name), None, "{name}");
    }
}

/// A reader that remembers the largest buffer it was asked to fill.
struct Watched<R> {
    inner: R,
    largest: Arc<AtomicUsize>,
}

impl<R: AsyncRead + Unpin> AsyncRead for Watched<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.largest.fetch_max(buf.remaining(), Ordering::Relaxed);
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

/// The streaming contract behind "a 10 GB file costs constant memory": however long the
/// stream, it is read through the one buffer of `BUFFER_SIZE`.
#[tokio::test]
async fn a_long_stream_is_read_through_one_fixed_buffer() {
    let len = 64 * BUFFER_SIZE as u64 + 3;
    let largest = Arc::new(AtomicUsize::new(0));
    let reader = Watched {
        inner: tokio::io::repeat(0x5a).take(len),
        largest: Arc::clone(&largest),
    };
    let control = Control::new();
    let hex = hash_stream(Box::new(reader), Algorithm::Crc32, Path::new("s"), &control)
        .await
        .expect("hashed");
    assert_eq!(hex.len(), 8);
    assert_eq!(control.bytes_done(), len);
    assert_eq!(largest.load(Ordering::Relaxed), BUFFER_SIZE);
}

#[tokio::test]
async fn a_cancelled_run_stops_with_cancelled() {
    let control = Control::new();
    control.cancel();
    let reader = tokio::io::repeat(1).take(1024);
    let error = hash_stream(Box::new(reader), Algorithm::Md5, Path::new("s"), &control)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code(), "cancelled");
}

#[tokio::test]
async fn hash_file_streams_a_file_of_the_vfs() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "abc.txt", b"abc");
    let hex = hash_file(
        &vfs,
        &root.path().join("abc.txt"),
        Algorithm::Sha256,
        &Control::new(),
    )
    .await
    .expect("hashed");
    assert_eq!(
        hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let missing = hash_file(
        &vfs,
        &root.path().join("nope"),
        Algorithm::Sha256,
        &Control::new(),
    )
    .await
    .expect_err("missing");
    assert_eq!(missing.code(), "not_found");
}

/// RC-080-42: the run's guard - links followed, a database probed - judges on the blocking pool,
/// not on the thread of the runtime; and still refuses.
#[tokio::test]
async fn the_guard_judges_off_the_runtime_thread() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "id_rsa", b"key");
    let runtime_thread = std::thread::current().id();
    let judged_on = Arc::new(std::sync::Mutex::new(None));
    let seen = Arc::clone(&judged_on);
    let control = Control::new().with_guard(Arc::new(move |path: &Path| {
        *seen.lock().expect("lock") = Some(std::thread::current().id());
        (path.file_name()? == "id_rsa").then_some("a key")
    }));
    let refused = hash_file(
        &vfs,
        &root.path().join("id_rsa"),
        Algorithm::Sha256,
        &control,
    )
    .await
    .expect_err("refused");
    assert_eq!(refused.code(), "sensitive_path");
    let thread = judged_on.lock().expect("lock").expect("judged");
    assert_ne!(thread, runtime_thread);
}
