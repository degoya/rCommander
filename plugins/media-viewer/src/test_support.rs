//! Sources and sinks for the tests: a file in memory or a large one made of a few pieces,
//! counting what is read from it.

use crate::types::{Failure, FailureKind, Sink, Source};

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
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.len());
        let end = start.saturating_add(length).min(self.len());
        self[start..end].to_vec()
    }
}

/// `head`, then `gap` zero bytes never held, then `tail`: a file of many GiB in a few hundred
/// bytes.
pub(crate) struct Sparse {
    pub(crate) head: Vec<u8>,
    pub(crate) gap: u64,
    pub(crate) tail: Vec<u8>,
}

impl Content for Sparse {
    fn size(&self) -> u64 {
        self.head.len() as u64 + self.gap + self.tail.len() as u64
    }

    fn slice(&self, offset: u64, length: usize) -> Vec<u8> {
        let end = (offset + length as u64).min(self.size());
        let head = self.head.len() as u64;
        let mut out = Vec::new();
        let mut at = offset;
        while at < end {
            if at < head {
                let take = (end.min(head) - at) as usize;
                out.extend_from_slice(&self.head[at as usize..at as usize + take]);
                at += take as u64;
            } else if at < head + self.gap {
                let take = end.min(head + self.gap) - at;
                out.resize(out.len() + take as usize, 0);
                at += take;
            } else {
                let from = (at - head - self.gap) as usize;
                let take = (end - at) as usize;
                out.extend_from_slice(&self.tail[from..from + take]);
                at += take as u64;
            }
        }
        out
    }
}

/// A source over `content` that counts its reads and can be told to stop.
pub(crate) struct TestSource<C: Content> {
    pub(crate) content: C,
    /// `info(src).name`.
    pub(crate) name: String,
    pub(crate) reads: u64,
    pub(crate) bytes_read: u64,
    /// `should-stop` answers true after this many reads.
    pub(crate) stop_after: Option<u64>,
}

impl<C: Content> TestSource<C> {
    pub(crate) fn new(content: C, name: &str) -> Self {
        Self {
            content,
            name: name.to_owned(),
            reads: 0,
            bytes_read: 0,
            stop_after: None,
        }
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
        Ok(bytes)
    }

    fn should_stop(&mut self) -> bool {
        self.stop_after.is_some_and(|after| self.reads >= after)
    }
}

/// A source over `bytes` named `name`.
pub(crate) fn named(bytes: &[u8], name: &str) -> TestSource<Vec<u8>> {
    TestSource::new(bytes.to_vec(), name)
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

/// The value of the fact `label` under the section `section` of `sections`.
pub(crate) fn fact<'a>(
    sections: &'a [crate::outline::Node],
    section: &str,
    label: &str,
) -> Option<&'a str> {
    sections
        .iter()
        .find(|node| node.label == section)?
        .find(label)?
        .value
        .as_deref()
}
