//! Sources, sinks and zips for the tests: a file in memory counting what is read from it, and
//! packages written with zip's own writer.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::out::Lines;
use crate::types::{Failure, FailureKind, Sink, Source};

/// The most one `read-at` or `write-at` moves (`docs/plugins.md`, "Shared interfaces").
pub(crate) const MAX_WINDOW: u32 = 1 << 20;

/// A source over bytes in memory that counts its reads and can be told to stop.
pub(crate) struct TestSource {
    pub(crate) content: Vec<u8>,
    pub(crate) reads: u64,
    pub(crate) bytes_read: u64,
    /// The lowest and highest offset read, to tell which parts were touched.
    pub(crate) touched: Vec<(u64, u64)>,
    /// `should-stop` answers true after this many reads.
    pub(crate) stop_after: Option<u64>,
}

impl TestSource {
    pub(crate) fn new(content: Vec<u8>) -> Self {
        Self {
            content,
            reads: 0,
            bytes_read: 0,
            touched: Vec::new(),
            stop_after: None,
        }
    }
}

impl Source for TestSource {
    fn size(&self) -> u64 {
        self.content.len() as u64
    }

    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure> {
        assert!(length <= MAX_WINDOW, "read-at of {length} bytes");
        if offset > self.content.len() as u64 {
            return Err(Failure::of(FailureKind::BadData));
        }
        let start = offset as usize;
        let end = (start + length as usize).min(self.content.len());
        let bytes = self.content[start..end].to_vec();
        self.reads += 1;
        self.bytes_read += bytes.len() as u64;
        self.touched.push((offset, offset + bytes.len() as u64));
        Ok(bytes)
    }

    fn should_stop(&mut self) -> bool {
        self.stop_after.is_some_and(|after| self.reads >= after)
    }
}

/// A sink in memory.
#[derive(Default)]
pub(crate) struct VecSink(pub(crate) Vec<u8>);

impl Sink for VecSink {
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> Result<(), Failure> {
        assert!(
            bytes.len() <= MAX_WINDOW as usize,
            "write-at of {} bytes",
            bytes.len()
        );
        let start = offset as usize;
        if self.0.len() < start + bytes.len() {
            self.0.resize(start + bytes.len(), 0);
        }
        self.0[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
}

/// Lines collected into one string.
#[derive(Default)]
pub(crate) struct Collect(pub(crate) String);

impl Lines for Collect {
    fn push(&mut self, text: &str) -> Result<(), Failure> {
        self.0.push_str(text);
        Ok(())
    }

    fn full(&self) -> bool {
        false
    }
}

/// A zip of `parts`, each deflated.
pub(crate) fn zip_of(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let methods: Vec<(&str, &[u8], CompressionMethod)> = parts
        .iter()
        .map(|(name, bytes)| (*name, *bytes, CompressionMethod::Deflated))
        .collect();
    zip_with(&methods)
}

/// A zip of `parts`, each packed as it says.
pub(crate) fn zip_with(parts: &[(&str, &[u8], CompressionMethod)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes, method) in parts {
        let options = SimpleFileOptions::default()
            .compression_method(*method)
            .large_file(bytes.len() as u64 >= u64::from(u32::MAX));
        writer.start_file(*name, options).expect("start a part");
        writer.write_all(bytes).expect("write a part");
    }
    writer.finish().expect("finish the zip").into_inner()
}
