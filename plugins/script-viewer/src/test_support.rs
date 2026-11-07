//! Sources and sinks for the tests: a file in memory or generated on the fly, counting what is
//! read from it; and a file's lines lexed, each token as its text.

use crate::lang::Lang;
use crate::lexer::{LineState, lex_line};
use crate::types::{Failure, FailureKind, Sink, Source, Token, TokenClass};

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

/// A source over `content`, named `name`, that counts its reads and can be told to stop.
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
            name: "main.rb".to_owned(),
            reads: 0,
            bytes_read: 0,
            largest_read: 0,
            stop_after: None,
        }
    }

    /// The same source under `name`.
    pub(crate) fn named(mut self, name: &str) -> Self {
        name.clone_into(&mut self.name);
        self
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

/// A source over `text`, named `main.rb`.
pub(crate) fn source(text: &str) -> TestSource<Vec<u8>> {
    TestSource::new(text.as_bytes().to_vec())
}

/// A source over `text`, named `name`.
pub(crate) fn named(name: &str, text: &str) -> TestSource<Vec<u8>> {
    source(text).named(name)
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

/// `text` in UTF-16 LE, after a byte-order mark when `bom`.
pub(crate) fn utf16le(text: &str, bom: bool) -> Vec<u8> {
    let mark: &[u8] = if bom { b"\xFF\xFE" } else { b"" };
    mark.iter()
        .copied()
        .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
        .collect()
}

/// Every line of `text` lexed as `lang` from the file's start, each token as its text and class.
pub(crate) fn lexed(lang: Lang, text: &str) -> Vec<Vec<(&str, TokenClass)>> {
    let mut state = LineState::default();
    text.split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let mut tokens: Vec<Token> = Vec::new();
            state = lex_line(lang, line.as_bytes(), state, &mut tokens);
            tokens
                .iter()
                .map(|token| {
                    let start = token.start as usize;
                    (&line[start..start + token.len as usize], token.class)
                })
                .collect()
        })
        .collect()
}

/// The texts of the tokens of `class` in `lines`, in order.
pub(crate) fn texts_of<'a>(
    lines: &[Vec<(&'a str, TokenClass)>],
    class: TokenClass,
) -> Vec<&'a str> {
    lines
        .iter()
        .flatten()
        .filter(|(_, of)| *of == class)
        .map(|(text, _)| *text)
        .collect()
}

/// The class of the first token whose text is `text`, if one is.
pub(crate) fn class_of(lines: &[Vec<(&str, TokenClass)>], text: &str) -> Option<TokenClass> {
    lines
        .iter()
        .flatten()
        .find(|(of, _)| *of == text)
        .map(|(_, class)| *class)
}
