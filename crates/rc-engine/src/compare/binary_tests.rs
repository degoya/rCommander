use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::sync::CancellationToken;

use super::{BinaryDifference, CHUNK, ReadFailure, first_difference, first_difference_of};

/// Answers its bytes at most `step` at a time, as a network stream does.
struct Trickle {
    bytes: Vec<u8>,
    at: usize,
    step: usize,
}

impl AsyncRead for Trickle {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let len = self
            .step
            .min(buf.remaining())
            .min(self.bytes.len() - self.at);
        let at = self.at;
        buf.put_slice(&self.bytes[at..at + len]);
        self.at += len;
        Poll::Ready(Ok(()))
    }
}

fn trickle(bytes: &[u8], step: usize) -> Trickle {
    Trickle {
        bytes: bytes.to_vec(),
        at: 0,
        step,
    }
}

async fn compare(left: &[u8], right: &[u8]) -> Option<BinaryDifference> {
    first_difference(&mut &left[..], &mut &right[..], &CancellationToken::new())
        .await
        .expect("compares")
}

#[tokio::test]
async fn the_same_bytes_have_no_difference() {
    assert_eq!(compare(b"", b"").await, None);
    assert_eq!(compare(b"abc", b"abc").await, None);
    let big = vec![7_u8; CHUNK * 3 + 5];
    assert_eq!(compare(&big, &big).await, None);
}

#[tokio::test]
async fn the_first_differing_byte_is_reported_with_both_values() {
    assert_eq!(
        compare(b"abcdef", b"abXdef").await,
        Some(BinaryDifference {
            offset: 2,
            left: Some(b'c'),
            right: Some(b'X'),
        })
    );
}

#[tokio::test]
async fn a_shorter_file_differs_where_it_ends() {
    assert_eq!(
        compare(b"abc", b"abcd").await,
        Some(BinaryDifference {
            offset: 3,
            left: None,
            right: Some(b'd'),
        })
    );
    assert_eq!(
        compare(b"ab", b"").await,
        Some(BinaryDifference {
            offset: 0,
            left: Some(b'a'),
            right: None,
        })
    );
}

#[tokio::test]
async fn streams_answering_different_amounts_line_up() {
    let mut left_bytes = vec![1_u8; CHUNK * 2 + 17];
    let right_bytes = left_bytes.clone();
    left_bytes[CHUNK + 3] = 2;
    let difference = first_difference(
        &mut trickle(&left_bytes, 1000),
        &mut trickle(&right_bytes, 777),
        &CancellationToken::new(),
    )
    .await
    .expect("compares");
    assert_eq!(
        difference,
        Some(BinaryDifference {
            offset: (CHUNK + 3) as u64,
            left: Some(2),
            right: Some(1),
        })
    );
    let same = first_difference(
        &mut trickle(&right_bytes, 4096),
        &mut trickle(&right_bytes, 13),
        &CancellationToken::new(),
    )
    .await
    .expect("compares");
    assert_eq!(same, None);
}

#[tokio::test]
async fn a_cancelled_comparison_stops() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = first_difference(&mut &b"a"[..], &mut &b"a"[..], &cancel).await;
    assert!(matches!(result, Err(ReadFailure::Cancelled)));
}

/// Two files in hand compare as their streams do: equal, a differing byte, one ending first.
#[tokio::test]
async fn bytes_in_hand_compare_as_their_streams() {
    let big = vec![7_u8; CHUNK + 5];
    let mut changed = big.clone();
    changed[CHUNK + 1] = 8;
    let pairs: [(&[u8], &[u8]); 6] = [
        (b"", b""),
        (b"abc", b"abc"),
        (b"abcdef", b"abXdef"),
        (b"abc", b"abcd"),
        (b"abcd", b""),
        (&big, &changed),
    ];
    for (left, right) in pairs {
        assert_eq!(
            first_difference_of(left, right),
            compare(left, right).await,
            "{} and {} bytes",
            left.len(),
            right.len()
        );
    }
}
