use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::io::AsyncReadExt;

use super::{Feed, Wanted, lock};
use crate::{
    Vfs,
    archive::{
        ArchiveFormat, ArchiveFs, ArchiveLimits, ArchiveSettings,
        pipe::CHUNK,
        read::Reading,
        source::Source,
        test_support::{fixture, local, open, read},
    },
};

const TREES: &[&str] = &[
    "tree.zip",
    "tree.7z",
    "tree.tar",
    "tree.tar.gz",
    "tree.tar.xz",
    "tree.tar.bz2",
    "tree.tar.zst",
];

/// The id of the producer parked for `fs`, waiting a moment for one that is still parking.
async fn parked(fs: &ArchiveFs) -> Option<u64> {
    for _ in 0..200 {
        if let Some(id) = lock(&fs.sessions.slot).as_ref().map(|parked| parked.id) {
            return Some(id);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    None
}

/// Every file of the archive, in the archive's own order.
async fn files_in_order(fs: &ArchiveFs) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![PathBuf::from("/")];
    while let Some(dir) = dirs.pop() {
        for (item, position) in fs.list_ordered(&dir).await.expect("list") {
            let path = dir.join(&item.entry.name);
            if item.entry.kind == crate::EntryKind::Dir {
                dirs.push(path);
            } else if item.entry.kind == crate::EntryKind::File {
                files.push((position, path));
            }
        }
    }
    files.sort();
    files.into_iter().map(|(_, path)| path).collect()
}

/// Acceptance (RC-080-46): the files of an archive read in its order are served by one
/// producer - the archive is opened once and a solid block or a compressed tar walked once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entries_read_in_order_share_one_producer() {
    for name in TREES {
        let fs = open(name).await;
        let files = files_in_order(&fs).await;
        assert!(files.len() > 3, "{name}");
        let mut first = None;
        for file in &files {
            let path = file.to_string_lossy();
            let whole = read(&fs, &path).await;
            let size = fs.stat(file).await.expect("stat").size;
            assert_eq!(whole.len() as u64, size, "{name} {path}");
            let id = parked(&fs).await.expect("a parked producer");
            assert_eq!(
                *first.get_or_insert(id),
                id,
                "{name}: {path} by another producer"
            );
        }
    }
}

/// A read behind the parked position, or of another block, is served right as well.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_backwards_starts_over() {
    for name in TREES {
        let fs = open(name).await;
        let files = files_in_order(&fs).await;
        let expected = {
            let mut all = Vec::new();
            for file in &files {
                all.push(read(&fs, &file.to_string_lossy()).await);
            }
            all
        };
        for (file, bytes) in files.iter().zip(&expected).rev() {
            assert_eq!(&read(&fs, &file.to_string_lossy()).await, bytes, "{name}");
        }
    }
}

/// A reader that goes away mid-entry ends its producer: nothing parks, and the next read starts
/// a producer of its own that reads right.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_read_parks_nothing() {
    let fs = open("solid-large.7z").await;
    let mut stream = fs
        .open_read(Path::new("/first.bin"))
        .await
        .expect("open first.bin");
    let mut chunk = vec![0; CHUNK];
    stream.read_exact(&mut chunk).await.expect("a chunk");
    drop(stream);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(lock(&fs.sessions.slot).is_none());
    let second = read(&fs, "/second.bin").await;
    assert_eq!(second.len(), 32 << 20);
    assert!(
        second
            .iter()
            .enumerate()
            .take(1000)
            .all(|(at, &byte)| byte == (at % 253) as u8)
    );
}

/// A producer parked for an archive file that changed since is let go; the read follows the
/// new file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_changed_archive_lets_the_parked_producer_go() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("archive.zip");
    std::fs::copy(fixture("tree.zip"), &path).expect("copy");
    let fs = ArchiveFs::new(
        local(),
        &path,
        ArchiveFormat::Zip,
        ArchiveSettings::default(),
    );
    assert_eq!(read(&fs, "/hello.txt").await, b"hello archive\n");
    assert!(parked(&fs).await.is_some());
    std::fs::copy(fixture("zip64.zip"), &path).expect("copy over");
    assert_eq!(read(&fs, "/zip64.txt").await, b"zip64 entry\n");
}

/// Acceptance: a cancel stops the producer, also while it still unpacks its way towards the
/// entry - the first 32 MiB of a solid block are not unpacked for a reader that is gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reader_gone_before_its_entry_stops_the_unpacking() {
    let fs = open("solid-large.7z").await;
    let node = fs
        .tree()
        .await
        .expect("tree")
        .find(Path::new("/second.bin"))
        .expect("second.bin")
        .clone();
    let source = Source::new(local(), &fixture("solid-large.7z")).expect("source");
    let reading = Reading {
        source,
        format: ArchiveFormat::SevenZ,
        limits: ArchiveLimits::default(),
        password: None,
    };
    let producer = tokio::task::spawn_blocking(move || {
        let mut feed = Feed::lonely();
        let (request, reader) = feed.request(Wanted {
            index: node.index.expect("an index"),
            size: node.entry.size,
            path: PathBuf::from("/second.bin"),
            encrypted: false,
        });
        drop(reader);
        reading.serve(request, &mut feed);
    });
    tokio::time::timeout(Duration::from_secs(30), producer)
        .await
        .expect("the producer stops")
        .expect("join");
}
