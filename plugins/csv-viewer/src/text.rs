//! The text for the sink: the file after its byte-order mark, as UTF-8 whatever it was written
//! in, in windows of at most [`WRITE_CHUNK`].

use crate::OPEN_CHUNK;
use crate::encoding::Encoding;
use crate::reader::Reader;
use crate::types::{Failure, Sink, Source};
use rc_plugin_text::Decoder;

/// The most one `write-at` takes.
const WRITE_CHUNK: usize = 1 << 20;

/// Writes the file from `start` into `out`, decoded from `encoding`.
pub(crate) fn write<S: Source>(
    src: &mut S,
    start: u64,
    encoding: Encoding,
    out: &mut impl Sink,
) -> Result<(), Failure> {
    let size = src.size();
    let mut reader = Reader::new(src, start, OPEN_CHUNK);
    let mut decoder = Decoder::new(encoding);
    let mut text = String::new();
    let mut written = 0u64;
    let mut flush = |text: &mut String| -> Result<(), Failure> {
        for chunk in text.as_bytes().chunks(WRITE_CHUNK) {
            out.write_at(written, chunk)?;
            written += chunk.len() as u64;
        }
        text.clear();
        Ok(())
    };
    reader.copy_range(start, size, &mut |bytes| {
        decoder.push(bytes, &mut text);
        flush(&mut text)
    })?;
    decoder.finish(&mut text);
    flush(&mut text)
}
