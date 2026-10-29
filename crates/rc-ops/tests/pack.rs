//! Packing (RC-030-04): every format round trip through the archive backend (RC-030-01), the
//! levels, one archive per source, cancelling and "move to archive".

mod support;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_ops::{JobChange, JobKind, JobOptions, JobSpec, JobState, PackFormat, PackSettings};
use rc_vfs::{ArchiveFs, ArchiveSettings, LocalFs, Vfs, VfsPath};
use support::{Harness, payload};
use tokio::io::AsyncReadExt;

/// A name long enough that the full path passes Windows' 260 characters.
fn long_name() -> String {
    format!("{}.txt", "l".repeat(150))
}

/// `root/src`: files at the top and below, umlauts, a long path, an empty folder.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let files = vec![
        ("src/a.txt".to_owned(), b"alpha\n".to_vec()),
        ("src/deep/b.bin".to_owned(), payload(200_000, 7)),
        (
            "src/deep/Gr\u{f6}\u{df}e \u{e4}\u{f6}\u{fc}.txt".to_owned(),
            b"umlaut\n".to_vec(),
        ),
        (
            format!("src/{}/{}/{}", "x".repeat(60), "y".repeat(60), long_name()),
            b"long\n".to_vec(),
        ),
        ("src/empty.txt".to_owned(), Vec::new()),
    ];
    for (rel, bytes) in &files {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, bytes).expect("write");
    }
    fs::create_dir_all(root.join("src/hollow")).expect("mkdir");
    files
}

fn pack(sources: &[PathBuf], target: &Path, settings: PackSettings) -> JobSpec {
    JobSpec {
        kind: JobKind::Pack {
            sources: sources.iter().map(VfsPath::local).collect(),
            target: VfsPath::local(target),
            settings,
        },
        options: JobOptions::default(),
    }
}

async fn read_inside(archive: &Path, inner: &str) -> Vec<u8> {
    let outer: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    let fs = ArchiveFs::open(outer, archive, ArchiveSettings::default())
        .await
        .expect("open")
        .expect("an archive");
    let mut stream = fs
        .open_read(Path::new(inner))
        .await
        .unwrap_or_else(|error| panic!("{}: {inner}: {error}", archive.display()));
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.expect("read");
    bytes
}

/// Acceptance: a selection with folders in every format; each reads back byte for byte, umlauts
/// and long paths included.
#[tokio::test]
async fn every_format_round_trips() {
    let root = tempfile::tempdir().expect("tempdir");
    let files = tree(root.path());
    let harness = Harness::start();
    harness.local();
    for format in PackFormat::ALL {
        for solid in [true, false] {
            if !solid && format != PackFormat::SevenZ {
                continue;
            }
            let archive = root
                .path()
                .join(format!("out-{solid}.{}", format.extension()));
            let settings = PackSettings {
                format,
                solid,
                ..PackSettings::default()
            };
            let done = harness
                .run(pack(&[root.path().join("src")], &archive, settings))
                .await;
            assert_eq!(
                done.state,
                JobState::Completed,
                "{format:?}: {:?}",
                done.failures
            );
            assert!(done.failures.is_empty(), "{format:?}: {:?}", done.failures);
            for (rel, bytes) in &files {
                assert_eq!(&read_inside(&archive, rel).await, bytes, "{format:?} {rel}");
            }
        }
    }
}

/// Acceptance: level 0 and level 9 give measurably different sizes.
#[tokio::test]
async fn level_zero_stores_and_nine_packs() {
    let root = tempfile::tempdir().expect("tempdir");
    fs::write(root.path().join("text.txt"), "rCommander ".repeat(50_000)).expect("write");
    let harness = Harness::start();
    harness.local();
    for format in [
        PackFormat::Zip,
        PackFormat::SevenZ,
        PackFormat::TarGz,
        PackFormat::TarZst,
    ] {
        let mut sizes = Vec::new();
        for level in [0, 9] {
            let archive = root.path().join(format!("l{level}.{}", format.extension()));
            let settings = PackSettings {
                format,
                level,
                ..PackSettings::default()
            };
            let done = harness
                .run(pack(&[root.path().join("text.txt")], &archive, settings))
                .await;
            assert_eq!(done.state, JobState::Completed, "{format:?} {level}");
            sizes.push(fs::metadata(&archive).expect("archive").len());
        }
        assert!(sizes[0] > sizes[1] * 10, "{format:?}: {sizes:?}");
    }
}

/// Acceptance: "each file into its own archive" makes one archive per source.
#[tokio::test]
async fn each_source_into_its_own_archive() {
    let root = tempfile::tempdir().expect("tempdir");
    tree(root.path());
    let harness = Harness::start();
    harness.local();
    let out = root.path().join("out");
    let settings = PackSettings {
        each_own_archive: true,
        ..PackSettings::default()
    };
    let sources = [root.path().join("src/a.txt"), root.path().join("src/deep")];
    let done = harness.run(pack(&sources, &out, settings)).await;
    assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
    assert_eq!(
        read_inside(&out.join("a.txt.zip"), "a.txt").await,
        b"alpha\n"
    );
    let deep = read_inside(&out.join("deep.zip"), "deep/b.bin").await;
    assert_eq!(deep, payload(200_000, 7));
    assert_eq!(fs::read_dir(&out).expect("out").count(), 2);
}

/// Acceptance: a cancel leaves no file under the target's name, and no temporary file either.
#[tokio::test]
async fn a_cancel_leaves_nothing_behind() {
    let root = tempfile::tempdir().expect("tempdir");
    fs::write(root.path().join("big.bin"), payload(8 << 20, 5)).expect("write");
    let harness = Harness::start();
    harness.local();
    harness.on_event(|queue, event| {
        if let JobChange::Progress { progress } = &event.change
            && progress.bytes_done > 0
            && progress.bytes_done < progress.bytes_total
        {
            let _ = queue.cancel(event.job);
        }
    });
    let archive = root.path().join("big.7z");
    let settings = PackSettings {
        format: PackFormat::SevenZ,
        level: 1,
        ..PackSettings::default()
    };
    let done = harness
        .run(pack(&[root.path().join("big.bin")], &archive, settings))
        .await;
    assert_eq!(done.state, JobState::Cancelled);
    let left: Vec<_> = fs::read_dir(root.path())
        .expect("root")
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(left, ["big.bin"], "only the source stays");
}

/// Acceptance: "delete sources" removes them only after the archive is written; a pack that
/// fails keeps them.
#[tokio::test]
async fn sources_go_only_after_a_complete_archive() {
    let root = tempfile::tempdir().expect("tempdir");
    tree(root.path());
    let harness = Harness::start();
    harness.local();
    let archive = root.path().join("moved.tar.xz");
    fs::write(&archive, b"in the way").expect("write");
    let settings = PackSettings {
        format: PackFormat::TarXz,
        delete_sources: true,
        ..PackSettings::default()
    };
    let mut refused = pack(&[root.path().join("src")], &archive, settings);
    refused.options.on_error = rc_ops::ErrorPolicy::Skip;
    let failed = harness.run(refused).await;
    assert_eq!(failed.failures[0].code, "already_exists");
    assert!(
        root.path().join("src/a.txt").is_file(),
        "kept after a failed pack"
    );

    let settings = PackSettings {
        overwrite: true,
        ..settings
    };
    let done = harness
        .run(pack(&[root.path().join("src")], &archive, settings))
        .await;
    assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
    assert!(!root.path().join("src").exists(), "moved into the archive");
    assert_eq!(read_inside(&archive, "src/a.txt").await, b"alpha\n");
}

/// A backend on which `/log.txt` grows between its count and its read.
struct Growing(rc_vfs::MemoryFs);

#[async_trait::async_trait]
impl rc_vfs::testing::Forward for Growing {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<rc_vfs::ReadStream> {
        let stream = self.0.open_read(path).await?;
        if path == Path::new("/log.txt") {
            return Ok(Box::new(stream.chain(&b" and more"[..])));
        }
        Ok(stream)
    }
}

/// RC-080-52: a file that grew while it was packed goes in with its counted size, is a failure,
/// and a move to the archive keeps it; the other sources go.
#[tokio::test]
async fn a_file_that_grew_is_a_failure_and_stays() {
    for format in [
        PackFormat::Zip,
        PackFormat::Tar,
        PackFormat::TarZst,
        PackFormat::SevenZ,
    ] {
        let root = tempfile::tempdir().expect("tempdir");
        let harness = Harness::start();
        harness.local();
        let memory = rc_vfs::MemoryFs::new();
        support::write(&memory, "/log.txt", b"counted").await;
        support::write(&memory, "/other.txt", b"other").await;
        harness.queue.register(
            rc_vfs::BackendId::Memory(8),
            rc_ops::Backend::new(Arc::new(Growing(memory.clone()))),
        );
        let archive = root.path().join(format!("moved.{}", format.extension()));
        let spec = JobSpec {
            kind: JobKind::Pack {
                sources: vec![support::mem(8, "/log.txt"), support::mem(8, "/other.txt")],
                target: VfsPath::local(&archive),
                settings: PackSettings {
                    format,
                    delete_sources: true,
                    ..PackSettings::default()
                },
            },
            options: JobOptions {
                on_error: rc_ops::ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        };
        let done = harness.run(spec).await;
        let codes: Vec<&str> = done.failures.iter().map(|f| f.code.as_str()).collect();
        assert_eq!(codes, ["verify_failed"], "{format:?}");
        assert!(
            support::exists(&memory, "/log.txt").await,
            "{format:?}: kept"
        );
        assert!(
            !support::exists(&memory, "/other.txt").await,
            "{format:?}: moved"
        );
        assert_eq!(
            read_inside(&archive, "log.txt").await,
            b"counted",
            "{format:?}"
        );
    }
}

/// Acceptance: tar archives unpack with the system's `tar` too, byte for byte.
#[cfg(unix)]
#[tokio::test]
async fn the_system_tar_reads_them() {
    let root = tempfile::tempdir().expect("tempdir");
    let files = tree(root.path());
    let harness = Harness::start();
    harness.local();
    for (format, flag) in [
        (PackFormat::Tar, "-xf"),
        (PackFormat::TarGz, "-xzf"),
        (PackFormat::TarBz2, "-xjf"),
        (PackFormat::TarXz, "-xJf"),
    ] {
        let archive = root.path().join(format!("sys.{}", format.extension()));
        let settings = PackSettings {
            format,
            ..PackSettings::default()
        };
        let done = harness
            .run(pack(&[root.path().join("src")], &archive, settings))
            .await;
        assert_eq!(done.state, JobState::Completed, "{format:?}");
        let out = root.path().join(format!("sys-{}", format.extension()));
        fs::create_dir_all(&out).expect("mkdir");
        let Ok(status) = std::process::Command::new("tar")
            .arg(flag)
            .arg(&archive)
            .arg("-C")
            .arg(&out)
            .status()
        else {
            eprintln!("no tar on this machine: skipped");
            return;
        };
        assert!(status.success(), "{format:?}");
        for (rel, bytes) in &files {
            assert_eq!(
                &fs::read(out.join(rel)).expect(rel),
                bytes,
                "{format:?} {rel}"
            );
        }
    }
}

/// A backend whose files panic when read: an archive crate (or a backend) that fails hard.
struct Panicking(rc_vfs::MemoryFs);

struct PanickingReader;

impl tokio::io::AsyncRead for PanickingReader {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        panic!("a reader that fails hard");
    }
}

#[async_trait::async_trait]
impl rc_vfs::testing::Forward for Panicking {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    async fn open_read(&self, _path: &Path) -> rc_vfs::Result<rc_vfs::ReadStream> {
        Ok(Box::new(PanickingReader))
    }
}

/// A panic while an archive is written - in the blocking task (zip) or in the tar thread of
/// tar.zst - fails that archive with `pack_failed`, leaves no file behind, and the queue (the
/// app) goes on.
#[tokio::test]
async fn a_panicking_writer_fails_the_archive_not_the_app() {
    for format in [PackFormat::Zip, PackFormat::TarZst, PackFormat::SevenZ] {
        let root = tempfile::tempdir().expect("tempdir");
        let harness = Harness::start();
        harness.local();
        let memory = rc_vfs::MemoryFs::new();
        support::write(&memory, "/boom.bin", b"data").await;
        harness.queue.register(
            rc_vfs::BackendId::Memory(7),
            rc_ops::Backend::new(Arc::new(Panicking(memory))),
        );
        let archive = root.path().join(format!("boom.{}", format.extension()));
        let spec = JobSpec {
            kind: JobKind::Pack {
                sources: vec![support::mem(7, "/boom.bin")],
                target: VfsPath::local(&archive),
                settings: PackSettings {
                    format,
                    ..PackSettings::default()
                },
            },
            options: JobOptions {
                on_error: rc_ops::ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        };
        let done = harness.run(spec).await;
        assert_eq!(done.state, JobState::Completed, "{format:?}");
        let codes: Vec<&str> = done.failures.iter().map(|f| f.code.as_str()).collect();
        assert_eq!(codes, ["pack_failed"], "{format:?}");
        let left = fs::read_dir(root.path()).expect("root").count();
        assert_eq!(
            left, 0,
            "{format:?}: nothing under the target, no temporary file"
        );
    }
}
