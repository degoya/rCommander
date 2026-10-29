use std::{
    error::Error,
    fmt,
    io::{self, Read, Seek, SeekFrom},
};

use super::{Source, io_error};
use crate::{VfsError, archive::test_support::local};

/// Whatever the order of seeks, the reader returns what the file holds there - inside its
/// buffer, across its end, and past the end of the file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_seekable_reader_reads_what_the_file_holds_wherever_it_seeks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("data.bin");
    let data: Vec<u8> = (0..200_000_u32).map(|index| (index % 251) as u8).collect();
    std::fs::write(&path, &data).expect("write");
    let source = Source::new(local(), &path).expect("source");
    let checked = tokio::task::spawn_blocking(move || {
        let mut reader = source.seekable().expect("open");
        let steps = [
            (SeekFrom::Start(0), 10),
            (SeekFrom::Current(5), 100),
            (SeekFrom::Start(65_530), 20),
            (SeekFrom::Start(100_000), 70_000),
            (SeekFrom::Current(-1_000), 10),
            (SeekFrom::End(-5), 10),
            (SeekFrom::Start(3), 3),
            (SeekFrom::End(10), 10),
        ];
        for (step, (to, len)) in steps.into_iter().enumerate() {
            let at = reader.seek(to).expect("seek");
            let mut got = Vec::new();
            (&mut reader).take(len).read_to_end(&mut got).expect("read");
            let start = usize::try_from(at).expect("small").min(data.len());
            let end = (start + usize::try_from(len).expect("small")).min(data.len());
            assert_eq!(got, data[start..end], "step {step} at {at}");
        }
        let before = reader.seek(SeekFrom::Current(-1_000_000));
        assert_eq!(
            before.map_err(|error| error.kind()),
            Err(io::ErrorKind::InvalidInput)
        );
    });
    checked.await.expect("the checks");
}

/// An error that wraps an I/O error, as the tar crate wraps the ones it passes on.
#[derive(Debug)]
struct Wrapper(io::Error);

impl fmt::Display for Wrapper {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "wrapped: {}", self.0)
    }
}

impl Error for Wrapper {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

#[test]
fn a_limit_raised_deep_in_a_crate_stays_a_limit() {
    let limit = VfsError::ArchiveLimit {
        path: "a.tar.gz".into(),
        limit: "compression ratio",
    };
    let wrapped = io::Error::other(Wrapper(limit.into_io()));
    let error = io_error(wrapped, std::path::Path::new("a.tar.gz"));
    assert_eq!(error.code(), "archive_limit");
}

#[test]
fn invalid_or_truncated_data_is_a_damaged_archive() {
    for kind in [io::ErrorKind::InvalidData, io::ErrorKind::UnexpectedEof] {
        let error = io_error(io::Error::new(kind, "bad"), std::path::Path::new("a.zip"));
        assert_eq!(error.code(), "archive_damaged", "{kind:?}");
    }
    let other = io_error(
        io::Error::from(io::ErrorKind::PermissionDenied),
        std::path::Path::new("a.zip"),
    );
    assert_eq!(other.code(), "permission_denied");
}
