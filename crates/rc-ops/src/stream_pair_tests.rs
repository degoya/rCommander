use std::{
    io,
    path::Path,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use rc_vfs::ReadStream;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use super::*;
use crate::{
    OpsError,
    control::Control,
    runner::{Step, Stop},
};

const SIZE: usize = 1024;

/// A source that counts the bytes it gave.
struct Counting<R> {
    inner: R,
    given: Arc<AtomicU64>,
}

impl<R: AsyncRead + Unpin> AsyncRead for Counting<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buffer.filled().len();
        let polled = Pin::new(&mut self.inner).poll_read(context, buffer);
        let count = (buffer.filled().len() - before) as u64;
        self.given.fetch_add(count, Ordering::SeqCst);
        polled
    }
}

/// A source whose link breaks.
struct Broken;

impl AsyncRead for Broken {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "the test link broke",
        )))
    }
}

fn counting(len: usize) -> (ReadStream, Arc<AtomicU64>) {
    let given = Arc::new(AtomicU64::new(0));
    let reader = Counting {
        inner: tokio::io::repeat(7).take(len as u64),
        given: Arc::clone(&given),
    };
    (Box::new(reader), given)
}

fn data(len: usize) -> Vec<u8> {
    (0..len).map(|index| (index % 251) as u8).collect()
}

/// What `drain` delivers, kept in `got`, each buffer given back.
async fn collect(drain: &mut Drain, got: &mut Vec<u8>) -> Result<(), Step> {
    while let Some(chunk) = drain.next().await {
        got.extend_from_slice(chunk.bytes());
        drain.give_back(chunk);
    }
    Ok(())
}

#[tokio::test]
async fn a_target_that_takes_nothing_stops_the_source_after_the_buffers() {
    let (feed, mut drain) = pair(SIZE, &Buffers::default());
    let (mut reader, given) = counting(100 * SIZE);
    let control = Control::default();
    let fill = feed.fill(&mut reader, &control, Path::new("/src"));
    tokio::pin!(fill);

    let waited = tokio::time::timeout(Duration::from_millis(50), &mut fill).await;
    assert!(waited.is_err(), "the feed waits for a free buffer");
    assert_eq!(given.load(Ordering::SeqCst), (DEPTH * SIZE) as u64);

    let chunk = drain.next().await.expect("a filled buffer");
    drain.give_back(chunk);
    let waited = tokio::time::timeout(Duration::from_millis(50), &mut fill).await;
    assert!(waited.is_err(), "the feed waits again");
    assert_eq!(
        given.load(Ordering::SeqCst),
        ((DEPTH + 1) * SIZE) as u64,
        "one buffer back, one read more"
    );
}

#[tokio::test]
async fn every_byte_arrives_in_order() {
    let expected = data(10 * SIZE + 17);
    let mut reader: ReadStream = Box::new(io::Cursor::new(expected.clone()));
    let (feed, mut drain) = pair(SIZE, &Buffers::default());
    let control = Control::default();
    let mut got = Vec::new();
    let read = feed.fill(&mut reader, &control, Path::new("/src"));
    both(read, collect(&mut drain, &mut got))
        .await
        .expect("copied");
    assert_eq!(got, expected);
}

/// RC-080-45: the files of one job take turns with the same buffers - the next file makes none.
#[tokio::test]
async fn the_next_file_takes_the_buffers_the_last_one_left() {
    let buffers = Buffers::default();
    let mut seen: Option<Vec<usize>> = None;
    for _ in 0..3 {
        let expected = data(10 * SIZE + 17);
        let mut reader: ReadStream = Box::new(io::Cursor::new(expected.clone()));
        let (feed, mut drain) = pair(SIZE, &buffers);
        let control = Control::default();
        let mut got = Vec::new();
        let read = feed.fill(&mut reader, &control, Path::new("/src"));
        both(read, collect(&mut drain, &mut got))
            .await
            .expect("copied");
        drop(drain);
        assert_eq!(got, expected);
        let mut kept: Vec<usize> = buffers
            .kept()
            .iter()
            .map(|bytes| bytes.as_ptr() as usize)
            .collect();
        kept.sort_unstable();
        assert!((1..=DEPTH).contains(&kept.len()), "{kept:?}");
        if let Some(seen) = &seen {
            assert_eq!(&kept, seen, "the same buffers again");
        }
        seen = Some(kept);
    }
}

#[tokio::test]
async fn a_failing_source_lets_the_target_take_what_came_before() {
    let before = data(3 * SIZE);
    let mut reader: ReadStream = Box::new(io::Cursor::new(before.clone()).chain(Broken));
    let (feed, mut drain) = pair(SIZE, &Buffers::default());
    let control = Control::default();
    let mut got = Vec::new();
    let read = feed.fill(&mut reader, &control, Path::new("/src"));
    let outcome = both(read, collect(&mut drain, &mut got)).await;
    assert!(matches!(outcome, Err(Step::Fail(_))), "{outcome:?}");
    assert_eq!(got, before, "every byte read before the break was written");
}

#[tokio::test]
async fn a_failing_target_stops_the_source_at_once() {
    let (mut reader, given) = counting(usize::MAX >> 1);
    let (feed, mut drain) = pair(SIZE, &Buffers::default());
    let control = Control::default();
    let read = feed.fill(&mut reader, &control, Path::new("/src"));
    let write = async {
        let _first = drain.next().await;
        Err::<(), Step>(OpsError::from_io(io::Error::other("the target went"), "/dst").into())
    };
    let outcome = tokio::time::timeout(Duration::from_secs(5), both(read, write))
        .await
        .expect("both end");
    assert!(matches!(outcome, Err(Step::Fail(_))), "{outcome:?}");
    assert!(given.load(Ordering::SeqCst) <= ((DEPTH + 1) * SIZE) as u64);
}

#[tokio::test]
async fn a_cancel_stops_the_source() {
    let (mut reader, _given) = counting(usize::MAX >> 1);
    let (feed, _drain) = pair(SIZE, &Buffers::default());
    let control = Control::default();
    control.cancel();
    let outcome = feed.fill(&mut reader, &control, Path::new("/src")).await;
    assert!(
        matches!(outcome, Err(Step::Stop(Stop::Cancelled))),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_feed_without_its_drain_ends_quietly() {
    let (mut reader, _given) = counting(100 * SIZE);
    let (feed, drain) = pair(SIZE, &Buffers::default());
    drop(drain);
    let control = Control::default();
    let outcome = feed.fill(&mut reader, &control, Path::new("/src")).await;
    assert!(outcome.is_ok(), "the write side has the reason");
}
