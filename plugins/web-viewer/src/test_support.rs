//! Sources and sinks for the tests: a named file in memory or generated on the fly, counting
//! what is read from it; the tokens of a text as the classes and text they cover.

use crate::Viewer;
use crate::types::{Failure, FailureKind, Sink, Source, TokenClass};

/// The most one `read-at` or `write-at` moves (`docs/plugins.md`, "Shared interfaces").
pub(crate) const MAX_WINDOW: u32 = 1 << 20;

/// What a test source serves.
pub(crate) trait Content {
    fn size(&self) -> u64;
    fn slice(&self, offset: u64, length: usize) -> Vec<u8>;
}

impl Content for Vec<u8> {
    fn size(&self) -> u64 {
        self.len() as u64
    }

    fn slice(&self, offset: u64, length: usize) -> Vec<u8> {
        let start = (offset as usize).min(self.len());
        let end = (start + length).min(self.len());
        self[start..end].to_vec()
    }
}

/// `head`, then `body` `times` times, then `tail`, never held whole: a 100 MiB file in a few
/// hundred bytes.
pub(crate) struct Repeated {
    pub(crate) head: Vec<u8>,
    pub(crate) body: Vec<u8>,
    pub(crate) times: u64,
    pub(crate) tail: Vec<u8>,
}

impl Content for Repeated {
    fn size(&self) -> u64 {
        self.head.len() as u64 + self.body.len() as u64 * self.times + self.tail.len() as u64
    }

    fn slice(&self, offset: u64, length: usize) -> Vec<u8> {
        let end = (offset + length as u64).min(self.size());
        let head = self.head.len() as u64;
        let body = self.body.len() as u64 * self.times;
        let mut out = Vec::with_capacity(length);
        let mut at = offset;
        while at < end {
            let piece: &[u8] = if at < head {
                &self.head[at as usize..]
            } else if at < head + body {
                &self.body[((at - head) % self.body.len() as u64) as usize..]
            } else {
                &self.tail[(at - head - body) as usize..]
            };
            let take = piece.len().min((end - at) as usize);
            out.extend_from_slice(&piece[..take]);
            at += take as u64;
        }
        out
    }
}

/// A source over `content` that counts its reads and can be told to stop.
pub(crate) struct TestSource<C: Content> {
    pub(crate) content: C,
    pub(crate) name: String,
    pub(crate) reads: u64,
    pub(crate) bytes_read: u64,
    pub(crate) largest_read: u32,
    /// `should-stop` answers true after this many reads.
    pub(crate) stop_after: Option<u64>,
}

impl<C: Content> TestSource<C> {
    pub(crate) fn new(content: C) -> Self {
        Self {
            content,
            name: String::new(),
            reads: 0,
            bytes_read: 0,
            largest_read: 0,
            stop_after: None,
        }
    }

    pub(crate) fn reset_counts(&mut self) {
        self.reads = 0;
        self.bytes_read = 0;
        self.largest_read = 0;
    }
}

impl<C: Content> Source for TestSource<C> {
    fn size(&self) -> u64 {
        self.content.size()
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure> {
        assert!(length <= MAX_WINDOW, "read-at of {length} bytes");
        if offset > self.content.size() {
            return Err(Failure::of(FailureKind::BadData));
        }
        let bytes = self.content.slice(offset, length as usize);
        self.reads += 1;
        self.bytes_read += bytes.len() as u64;
        self.largest_read = self.largest_read.max(length);
        Ok(bytes)
    }

    fn should_stop(&mut self) -> bool {
        self.stop_after.is_some_and(|after| self.reads >= after)
    }
}

/// A source over `bytes`, the file named `name`.
pub(crate) fn named(bytes: &[u8], name: &str) -> TestSource<Vec<u8>> {
    let mut src = TestSource::new(bytes.to_vec());
    src.name = name.to_owned();
    src
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

/// The tokens of every line of `text`, the file named `name`, as (class, the text they cover),
/// read from the text the viewer writes, so offsets after a U+FFFD count right.
pub(crate) fn highlighted(name: &str, text: &[u8]) -> Vec<Vec<(TokenClass, String)>> {
    let (mut viewer, _) = Viewer::open(named(text, name)).expect("the document opens");
    let mut sink = VecSink::default();
    viewer.text(&mut sink).expect("text");
    let written = String::from_utf8(sink.0).expect("the text is UTF-8");
    let count = u32::try_from(viewer.line_count()).expect("a test has few lines");
    let tokens = viewer.tokens(0, count).expect("tokens");
    written
        .split('\n')
        .zip(tokens)
        .map(|(line, spans)| {
            spans
                .iter()
                .map(|span| {
                    let start = span.start as usize;
                    let text = line
                        .get(start..start + span.len as usize)
                        .expect("a span on character boundaries inside its line");
                    (span.class, text.to_owned())
                })
                .collect()
        })
        .collect()
}

/// The tokens of the one line `text` is, in the file named `name`.
pub(crate) fn line(name: &str, text: &str) -> Vec<(TokenClass, String)> {
    highlighted(name, text.as_bytes())
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// The tokens of every line of `text`, all in one list.
pub(crate) fn all(name: &str, text: &str) -> Vec<(TokenClass, String)> {
    highlighted(name, text.as_bytes())
        .into_iter()
        .flatten()
        .collect()
}

/// The class of the first token covering exactly `needle` among `tokens`.
pub(crate) fn class_of(tokens: &[(TokenClass, String)], needle: &str) -> Option<TokenClass> {
    tokens
        .iter()
        .find(|(_, text)| text == needle)
        .map(|(class, _)| *class)
}

/// Asserts that each of `expected` is a token of `tokens`: its text, and its class.
#[track_caller]
pub(crate) fn assert_tokens(tokens: &[(TokenClass, String)], expected: &[(TokenClass, &str)]) {
    for (class, needle) in expected {
        assert_eq!(
            class_of(tokens, needle),
            Some(*class),
            "{needle:?} in {tokens:?}"
        );
    }
}

/// Asserts that no token covers exactly `needle`.
#[track_caller]
pub(crate) fn assert_plain(tokens: &[(TokenClass, String)], needle: &str) {
    assert_eq!(class_of(tokens, needle), None, "{needle:?} in {tokens:?}");
}
