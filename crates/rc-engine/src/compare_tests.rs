use std::{
    fs, io,
    path::Path,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::SystemTime,
};

use async_trait::async_trait;
use rc_vfs::{
    Attributes, Capabilities, Entry, EntryKind, LocalFs, ReadStream, SetAttributes, Vfs, VfsError,
    WriteMode, WriteStream,
};
use tokio::io::{AsyncRead, ReadBuf};

use super::*;

/// Two files of [`PatternFs::SIZE`] bytes that exist only as a pattern: `/same` and `/other`,
/// which differs from it in one byte at [`PatternFs::DIFFERS_AT`]. Nothing is ever held whole.
struct PatternFs;

impl PatternFs {
    /// 4 GiB and a few bytes: past every 32-bit size.
    const SIZE: u64 = 4 * 1024 * 1024 * 1024 + 7;
    const DIFFERS_AT: u64 = Self::SIZE - 10;
}

struct Pattern {
    chunk: Arc<Vec<u8>>,
    at: u64,
    differs_at: Option<u64>,
}

impl AsyncRead for Pattern {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let left = PatternFs::SIZE - self.at;
        let len = buf
            .remaining()
            .min(self.chunk.len())
            .min(usize::try_from(left).unwrap_or(usize::MAX));
        let start = buf.filled().len();
        buf.put_slice(&self.chunk[..len]);
        if let Some(at) = self.differs_at
            && (self.at..self.at + len as u64).contains(&at)
        {
            buf.filled_mut()[start + (at - self.at) as usize] = b'!';
        }
        self.at += len as u64;
        Poll::Ready(Ok(()))
    }
}

fn unsupported<T>(path: &Path) -> rc_vfs::Result<T> {
    Err(VfsError::Unsupported {
        path: path.to_path_buf(),
        operation: "writing",
    })
}

#[async_trait]
impl Vfs for PatternFs {
    fn capabilities(&self) -> Capabilities {
        LocalFs::new().capabilities()
    }
    async fn list(&self, path: &Path) -> rc_vfs::Result<Vec<Entry>> {
        unsupported(path)
    }
    async fn stat(&self, path: &Path) -> rc_vfs::Result<Entry> {
        Ok(Entry {
            name: path.to_string_lossy().into_owned(),
            kind: EntryKind::File,
            size: Self::SIZE,
            modified: Some(SystemTime::UNIX_EPOCH),
            created: None,
            attributes: Attributes::default(),
            hidden: false,
            owner: None,
        })
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        Ok(Box::new(Pattern {
            chunk: Arc::new(vec![b'x'; 64 * 1024]),
            at: 0,
            differs_at: (path == Path::new("/other")).then_some(Self::DIFFERS_AT),
        }))
    }
    async fn open_write(&self, path: &Path, _: WriteMode) -> rc_vfs::Result<WriteStream> {
        unsupported(path)
    }
    async fn mkdir(&self, path: &Path, _: bool) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn rename(&self, from: &Path, _: &Path) -> rc_vfs::Result<()> {
        unsupported(from)
    }
    async fn remove_file(&self, path: &Path) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn remove_dir(&self, path: &Path, _: bool) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn set_mtime(&self, path: &Path, _: SystemTime) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn set_attributes(&self, path: &Path, _: SetAttributes) -> rc_vfs::Result<()> {
        unsupported(path)
    }
}

async fn compare_local(
    left: &Path,
    right: &Path,
    options: TextOptions,
    unified_limit: usize,
) -> Result<FileComparison, EngineError> {
    let local = LocalFs::new();
    let (left, right) = (VfsPath::local(left), VfsPath::local(right));
    compare_files(
        Side {
            vfs: &local,
            path: &left,
        },
        Side {
            vfs: &local,
            path: &right,
        },
        &options,
        unified_limit,
        &CancellationToken::new(),
    )
    .await
}

#[tokio::test]
async fn two_4_gib_files_compare_streamed_and_find_the_byte_near_the_end() {
    let fs = PatternFs;
    let (same, other) = (VfsPath::local("/same"), VfsPath::local("/other"));
    let side = |path| Side { vfs: &fs, path };
    let comparison = compare_files(
        side(&same),
        side(&other),
        &TextOptions::default(),
        4096,
        &CancellationToken::new(),
    )
    .await
    .expect("compares");
    assert!(!comparison.identical);
    assert_eq!(
        comparison.first_difference,
        Some(BinaryDifference {
            offset: PatternFs::DIFFERS_AT,
            left: Some(b'x'),
            right: Some(b'!'),
        })
    );
    assert_eq!(comparison.left_size, PatternFs::SIZE);
    // Far past the line diff's limit: the first byte is the whole answer.
    assert_eq!(comparison.text, None);
}

#[tokio::test]
async fn identical_files_are_identical() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("a"), b"same\n").expect("write");
    fs::write(dir.path().join("b"), b"same\n").expect("write");
    let comparison = compare_local(
        &dir.path().join("a"),
        &dir.path().join("b"),
        TextOptions::default(),
        1000,
    )
    .await
    .expect("compares");
    assert!(comparison.identical);
    assert_eq!(comparison.first_difference, None);
    assert_eq!(comparison.text, None);
}

#[tokio::test]
async fn text_files_get_a_line_comparison_and_a_unified_diff() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("a"), "one\ntwo\nthree\n").expect("write");
    fs::write(dir.path().join("b"), "one\nTWO\nthree\n").expect("write");
    let comparison = compare_local(
        &dir.path().join("a"),
        &dir.path().join("b"),
        TextOptions::default(),
        1000,
    )
    .await
    .expect("compares");
    assert_eq!(
        comparison
            .first_difference
            .map(|difference| difference.offset),
        Some(4)
    );
    let text = comparison.text.expect("a text comparison");
    assert!(!text.equal);
    assert_eq!(text.changes, 1);
    assert_eq!(text.first_change, Some(LinePosition { left: 2, right: 2 }));
    let unified = text.unified.expect("a unified diff");
    assert_eq!(unified.text, "@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n");

    let ignoring_case = compare_local(
        &dir.path().join("a"),
        &dir.path().join("b"),
        TextOptions {
            ignore_case: true,
            ..TextOptions::default()
        },
        0,
    )
    .await
    .expect("compares");
    let text = ignoring_case.text.expect("a text comparison");
    assert!(!ignoring_case.identical, "the bytes still differ");
    assert!(text.equal);
    assert_eq!(text.unified, None, "none asked for");
}

#[tokio::test]
async fn binary_files_get_only_the_first_byte() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("a"), b"MZ\0\x01").expect("write");
    fs::write(dir.path().join("b"), b"MZ\0\x02").expect("write");
    let comparison = compare_local(
        &dir.path().join("a"),
        &dir.path().join("b"),
        TextOptions::default(),
        1000,
    )
    .await
    .expect("compares");
    assert_eq!(
        comparison.first_difference,
        Some(BinaryDifference {
            offset: 3,
            left: Some(1),
            right: Some(2),
        })
    );
    assert_eq!(comparison.text, None);
}

#[tokio::test]
async fn a_directory_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("a"), b"a").expect("write");
    let error = compare_local(dir.path(), &dir.path().join("a"), TextOptions::default(), 0)
        .await
        .expect_err("a directory");
    assert_eq!(error.code(), "is_a_directory");
}
