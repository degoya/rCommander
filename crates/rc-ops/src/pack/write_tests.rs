use std::io::{self, Write};

use super::{PipeWriter, encode_zst, is_panic};

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
