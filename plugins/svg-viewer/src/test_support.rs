//! Sources, sinks and pictures for the tests: a file in memory counting its reads, a sink
//! gathering what is written, PNG and gzip made on the spot, a `<use>` bomb.

use std::io::Write;

use base64::Engine;
use flate2::Compression;
use flate2::write::{GzEncoder, ZlibEncoder};
use resvg::tiny_skia::Pixmap;

use crate::types::{Failure, Sink, Source};

/// A file in memory that counts its reads and can be told to stop.
pub(crate) struct TestSource {
    pub(crate) content: Vec<u8>,
    pub(crate) reads: u64,
    pub(crate) largest_read: u32,
    /// `should-stop` answers true after this many reads.
    pub(crate) stop_after: Option<u64>,
}

impl TestSource {
    pub(crate) fn new(content: Vec<u8>) -> Self {
        Self {
            content,
            reads: 0,
            largest_read: 0,
            stop_after: None,
        }
    }
}

impl Source for TestSource {
    fn size(&self) -> u64 {
        self.content.len() as u64
    }

    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure> {
        self.reads += 1;
        self.largest_read = self.largest_read.max(length);
        let start = (offset as usize).min(self.content.len());
        let end = (start + length as usize).min(self.content.len());
        Ok(self.content[start..end].to_vec())
    }

    fn should_stop(&mut self) -> bool {
        self.stop_after.is_some_and(|after| self.reads >= after)
    }
}

/// A sink that keeps what is written, in order.
#[derive(Default)]
pub(crate) struct VecSink(pub(crate) Vec<u8>);

impl Sink for VecSink {
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> Result<(), Failure> {
        assert_eq!(offset, self.0.len() as u64, "writes follow each other");
        assert!(bytes.len() <= 1 << 20, "a write is at most 1 MiB");
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}

/// `bytes` gzipped, as an `.svgz` is.
pub(crate) fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).expect("into memory");
    encoder.finish().expect("into memory")
}

/// `count` zero bytes gzipped, never held whole.
pub(crate) fn gzip_zeros(count: u64) -> Vec<u8> {
    let zeros = vec![0u8; 1 << 20];
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    let mut left = count;
    while left > 0 {
        let take = left.min(zeros.len() as u64) as usize;
        encoder.write_all(&zeros[..take]).expect("into memory");
        left -= take as u64;
    }
    encoder.finish().expect("into memory")
}

/// A PNG of `width` by `height` pixels, all `rgba`.
pub(crate) fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    // Each row: the filter byte (none), then the pixels.
    let mut row = vec![0u8];
    row.extend(rgba.repeat(width as usize));
    let raw = row.repeat(height as usize);
    let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
    zlib.write_all(&raw).expect("into memory");
    let idat = zlib.finish().expect("into memory");
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, RGBA, deflate, no filter, no interlace
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [(b"IHDR", &ihdr), (b"IDAT", &idat), (b"IEND", &Vec::new())] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut crc = flate2::Crc::new();
        crc.update(kind);
        crc.update(data);
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc.sum().to_be_bytes());
    }
    out
}

/// `bytes` as a `data:` URL of `mime`, base64.
pub(crate) fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// `levels` groups, each `<use>`-ing the one before `fanout` times: `fanout^levels` elements
/// once expanded, a few lines as written.
pub(crate) fn use_bomb(levels: u32, fanout: u32) -> String {
    let mut svg = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><defs><g id="l0"><rect width="1" height="1"/></g>"#,
    );
    for level in 1..=levels {
        svg.push_str(&format!(r#"<g id="l{level}">"#));
        for _ in 0..fanout {
            svg.push_str(&format!(r##"<use href="#l{}"/>"##, level - 1));
        }
        svg.push_str("</g>");
    }
    svg.push_str(&format!(r##"</defs><use href="#l{levels}"/></svg>"##));
    svg
}

/// The pixel at `x`, `y` as straight RGBA.
pub(crate) fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let color = pixmap.pixel(x, y).expect("inside the canvas").demultiply();
    [color.red(), color.green(), color.blue(), color.alpha()]
}

/// How many pixels are not fully transparent.
pub(crate) fn painted(pixmap: &Pixmap) -> usize {
    pixmap
        .pixels()
        .iter()
        .filter(|pixel| pixel.alpha() > 0)
        .count()
}

/// A picture `levels` elements deep, the root counted: `<g>` in `<g>` around one red square.
pub(crate) fn nested(levels: usize) -> String {
    let inner = levels.saturating_sub(1);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">{}<rect width="10" height="10" fill="red"/>{}</svg>"#,
        "<g>".repeat(inner),
        "</g>".repeat(inner)
    )
}
