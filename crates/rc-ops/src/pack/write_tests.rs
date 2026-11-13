use std::{
    io::{self, Read as _, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use rc_vfs::{LocalFs, ReadStream, Vfs};
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::watch,
};

use super::{Feed, Member, MemberKind, PipeWriter, encode_zst, is_panic};
use crate::control::Control;

/// A stream that never yields a byte: a server that stopped answering.
struct Never;

impl AsyncRead for Never {
    fn poll_read(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        _buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Pending
    }
}

/// The local disk, except that no file opened for reading ever yields anything.
struct Stalled(LocalFs);

#[async_trait::async_trait]
impl rc_vfs::testing::Forward for Stalled {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    async fn open_read(&self, _path: &Path) -> rc_vfs::Result<ReadStream> {
        Ok(Box::new(Never))
    }
}

/// A cancel ends the read of a source that stalls, not the backend's time limit (RC-110-26).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_ends_a_read_that_stalls() {
    let control = Control::default();
    let (bytes, _seen) = watch::channel(0_u64);
    let src: Arc<dyn Vfs> = Arc::new(Stalled(LocalFs::new()));
    let feed = Feed::new(src, control.clone(), bytes).expect("on the runtime");
    let reading = tokio::task::spawn_blocking(move || {
        let member = Member {
            path: PathBuf::from("/stalled.bin"),
            name: "stalled.bin".to_owned(),
            kind: MemberKind::File,
            size: 10,
            modified: None,
            mode: None,
            readonly: false,
        };
        let mut source = feed.open(&member)?;
        let mut buffer = [0_u8; 10];
        source.read(&mut buffer)
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!reading.is_finished(), "the read waits for the server");
    control.cancel();
    let ended = tokio::time::timeout(Duration::from_secs(10), reading)
        .await
        .expect("the cancel ended the read")
        .expect("joins");
    assert_eq!(
        ended.expect_err("cancelled").to_string(),
        "the job was cancelled"
    );
}

/// An output that fails after a few bytes, as a full disk does.
struct Failing {
    left: usize,
}

impl Write for Failing {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Err(io::Error::other("disk full"));
        }
        let count = buf.len().min(self.left);
        self.left -= count;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Plenty of data that does not compress well, so the output is written while it flows.
fn produce_noise(mut pipe: PipeWriter) -> io::Result<()> {
    let mut state: u32 = 7;
    for _ in 0..64 {
        let block: Vec<u8> = (0..65_536)
            .map(|_| {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (state >> 16) as u8
            })
            .collect();
        pipe.write_all(&block)?;
    }
    Ok(())
}

/// A failing output fails the archive with its own error: the encoder never panics on it, and
/// the producer stops instead of writing everything first.
#[test]
fn a_failing_output_is_an_error_not_a_panic() {
    for level in [0, 9] {
        let failed = encode_zst(Failing { left: 4096 }, level, produce_noise);
        let error = failed.err().expect("the output failed");
        assert_eq!(error.to_string(), "disk full", "level {level}");
    }
}

#[test]
fn a_panicking_producer_is_an_answer() {
    let produce = |_pipe: PipeWriter| -> io::Result<()> { panic!("an archive crate failed") };
    let failed = encode_zst(Vec::new(), 1, produce);
    assert!(failed.as_ref().is_err_and(is_panic), "{failed:?}");
}

#[test]
fn a_good_stream_round_trips() {
    let produce = |mut pipe: PipeWriter| pipe.write_all(&[42; 100_000]);
    let packed = encode_zst(Vec::new(), 9, produce).expect("packed");
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(packed.as_slice()).expect("zstd");
    let mut plain = Vec::new();
    io::Read::read_to_end(&mut decoder, &mut plain).expect("decoded");
    assert_eq!(plain, vec![42; 100_000]);
}
