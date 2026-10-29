use std::path::{Path, PathBuf};

use tokio::io::AsyncReadExt;

use crate::{
    Vfs,
    archive::{
        ArchiveLimits,
        pipe::CHUNK,
        session::{Feed, Wanted},
        source::Source,
        test_support::{code, fixture, local, open},
    },
};

/// Each entry of `solid-large.7z`: 32 MiB of a byte pattern.
const LARGE: u64 = 32 << 20;

fn second_byte(index: u64) -> u8 {
    (index % 253) as u8
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_content_lists_but_needs_a_password() {
    let fs = open("encrypted.7z").await;
    let item = fs
        .stat_detailed(Path::new("/secret.txt"))
        .await
        .expect("an encrypted block still lists");
    assert!(item.info.encrypted);
    assert_eq!(item.entry.size, 10);
    assert_eq!(
        code(fs.open_read(Path::new("/secret.txt")).await),
        "password_required"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_encrypted_header_needs_a_password_to_list() {
    let fs = open("encrypted-header.7z").await;
    assert_eq!(code(fs.list(Path::new("/")).await), "password_required");
}

/// The second entry of a solid block comes out whole and bit for bit, through the pipe.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_large_entry_of_a_solid_block_streams_whole() {
    let fs = open("solid-large.7z").await;
    let mut stream = fs
        .open_read(Path::new("/second.bin"))
        .await
        .expect("open second.bin");
    let mut buffer = vec![0; CHUNK];
    let mut offset = 0_u64;
    loop {
        let read = stream.read(&mut buffer).await.expect("read");
        if read == 0 {
            break;
        }
        for (at, &byte) in buffer[..read].iter().enumerate() {
            let index = offset + at as u64;
            assert_eq!(byte, second_byte(index), "byte {index}");
        }
        offset += read as u64;
    }
    assert_eq!(offset, LARGE);
}

/// Acceptance: an entry deep in a solid block streams, it is not unpacked into memory. The
/// reader takes one chunk and goes away; the producer stops there, mid-entry, instead of
/// unpacking the remaining 32 MiB.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reader_that_stops_stops_the_unpacking() {
    let fs = open("solid-large.7z").await;
    let tree = fs.tree().await.expect("tree");
    let node = tree.find(Path::new("/second.bin")).expect("second.bin");
    let index = node.index.expect("an index");
    assert!(
        node.info
            .packed_size
            .is_some_and(|packed| packed < LARGE / 100)
    );

    let source = Source::new(local(), &fixture("solid-large.7z")).expect("source");
    let mut feed = Feed::lonely();
    let (request, mut reader) = feed.request(Wanted {
        index,
        size: LARGE,
        path: PathBuf::from("/second.bin"),
        encrypted: false,
    });
    let producer = tokio::task::spawn_blocking(move || {
        super::serve(&source, &ArchiveLimits::default(), None, request, &mut feed);
    });
    let mut first = vec![0; CHUNK];
    reader
        .read_exact(&mut first)
        .await
        .expect("the first chunk");
    assert!(
        first
            .iter()
            .enumerate()
            .all(|(index, &byte)| byte == second_byte(index as u64))
    );
    drop(reader);
    tokio::time::timeout(std::time::Duration::from_secs(30), producer)
        .await
        .expect("the producer stops mid-entry")
        .expect("join");
}
