use std::{
    io::{self, Read},
    path::Path,
};

use tokio::io::AsyncReadExt;

use super::{CHUNK, PipeWriter, Pumped, pair, pump, spawn};
use crate::{VfsError, archive::test_support::code};

#[tokio::test]
async fn pump_sends_exactly_the_stated_size() {
    let (writer, mut reader) = pair(5);
    let producer =
        tokio::task::spawn_blocking(move || pump(&mut &b"hello"[..], 5, &writer, Path::new("/x")));
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.expect("read");
    assert_eq!(bytes, b"hello");
    assert_eq!(
        producer.await.expect("join").expect("pump"),
        Pumped::Complete
    );
}

/// A stated size that lies is what a bomb does: more bytes is a limit, fewer is damage.
#[tokio::test]
async fn a_size_that_lies_is_an_error() {
    let (writer, _reader) = pair(3);
    let more =
        tokio::task::spawn_blocking(move || pump(&mut &b"hello"[..], 3, &writer, Path::new("/x")));
    assert_eq!(code(more.await.expect("join")), "archive_limit");

    let (writer, _reader) = pair(9);
    let fewer =
        tokio::task::spawn_blocking(move || pump(&mut &b"hello"[..], 9, &writer, Path::new("/x")));
    assert_eq!(code(fewer.await.expect("join")), "archive_damaged");
}

/// The producer runs only a few chunks ahead and stops once the reader is gone.
#[tokio::test]
async fn a_reader_that_goes_away_stops_the_producer() {
    let size = 64 * CHUNK as u64;
    let (writer, reader) = pair(size);
    drop(reader);
    let producer = tokio::task::spawn_blocking(move || {
        pump(
            &mut io::repeat(7).take(size),
            size,
            &writer,
            Path::new("/x"),
        )
    });
    let pumped = producer.await.expect("join").expect("pump");
    assert!(
        matches!(pumped, Pumped::Abandoned { sent } if sent <= 5 * CHUNK as u64),
        "{pumped:?}"
    );
}

#[tokio::test]
async fn an_error_before_the_first_byte_fails_the_open() {
    let opened = spawn(Path::new("/x"), 10, |_: &PipeWriter| {
        Err(VfsError::PasswordRequired { path: "/x".into() })
    })
    .await;
    assert_eq!(code(opened), "password_required");
}

/// A producer that dies without a word leaves a stream that ends short: an error, never a
/// quietly truncated file.
#[tokio::test]
async fn a_producer_that_dies_is_no_end_of_file() {
    let opened = spawn(
        Path::new("/x"),
        10,
        |_: &PipeWriter| -> crate::Result<Pumped> { panic!("the producer died") },
    )
    .await;
    assert_eq!(code(opened), "io");
}

/// RC-080-70: the reader hands the chunks it has read back for the producer to fill again -
/// never more than the pipe's depth are kept - and every byte arrives as it was.
#[tokio::test]
async fn read_chunks_go_back_to_the_producer() {
    let size = 32 * CHUNK as u64 + 17;
    let source: Vec<u8> = (0..size).map(|index| (index % 251) as u8).collect();
    let (writer, mut reader) = pair(size);
    let bytes = source.clone();
    let producer =
        tokio::task::spawn_blocking(move || pump(&mut &bytes[..], size, &writer, Path::new("/x")));
    let mut got = Vec::new();
    reader.read_to_end(&mut got).await.expect("read");
    assert_eq!(
        producer.await.expect("join").expect("pump"),
        Pumped::Complete
    );
    assert_eq!(got, source);
    let kept = reader.spare.lock().expect("spare").len();
    assert!((1..=super::DEPTH).contains(&kept), "{kept} chunks kept");
}
