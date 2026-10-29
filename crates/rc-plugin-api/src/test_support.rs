//! An in-memory [`ViewerHost`] for a plugin's native tests.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-api/src/test_support.rs` at commit
//! 26c8592a6 (the scripted `ResolverHost` the plugins' native suites share): sources are byte
//! vectors, sinks collect what was written, log lines are kept, settings are a map. It holds the
//! host's promises a viewer may rely on - at most 1 MiB per read and write, a handle it was not
//! given refused - so a test against it finds what the real host would refuse.

use std::collections::BTreeMap;

use crate::viewer::ViewerHost;
use crate::{Failure, FailureKind, Sink, Source, SourceInfo};

/// The most one `read-at` or `write-at` moves, as the host enforces it.
pub const MAX_IO_BYTES: usize = 1024 * 1024;

/// Sources, sinks and log lines in memory.
#[derive(Debug, Default)]
pub struct MemoryHost {
    sources: BTreeMap<Source, (SourceInfo, Vec<u8>)>,
    sinks: BTreeMap<Sink, Vec<u8>>,
    /// Every line logged, as `(level, message)`.
    pub logs: Vec<(String, String)>,
    /// What `should-stop` answers.
    pub stop: bool,
    /// What `settings.setting` answers, by name: the manifest's defaults and the person's
    /// choices, as the host resolves them.
    pub settings: BTreeMap<String, String>,
}

impl MemoryHost {
    /// A host with no source and no sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Offers `bytes` as source `handle`, named `name`.
    pub fn add_source(&mut self, handle: Source, name: &str, bytes: Vec<u8>) {
        let info = SourceInfo {
            name: name.to_owned(),
            size: bytes.len() as u64,
            modified: None,
        };
        self.sources.insert(handle, (info, bytes));
    }

    /// Opens an empty sink `handle`.
    pub fn add_sink(&mut self, handle: Sink) {
        self.sinks.insert(handle, Vec::new());
    }

    /// What was written into sink `handle`.
    #[must_use]
    pub fn sink(&self, handle: Sink) -> Option<&[u8]> {
        self.sinks.get(&handle).map(Vec::as_slice)
    }
}

fn refused() -> Failure {
    Failure::coded(FailureKind::NotFound, "plugin_unknown_handle")
}

impl ViewerHost for MemoryHost {
    fn log(&mut self, level: &str, message: &str) {
        self.logs.push((level.to_owned(), message.to_owned()));
    }

    fn info(&mut self, src: Source) -> SourceInfo {
        self.sources
            .get(&src)
            .map(|(info, _)| info.clone())
            .unwrap_or(SourceInfo {
                name: String::new(),
                size: 0,
                modified: None,
            })
    }

    fn read_at(&mut self, src: Source, offset: u64, length: u32) -> Result<Vec<u8>, Failure> {
        let (_, bytes) = self.sources.get(&src).ok_or_else(refused)?;
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let length = (length as usize).min(MAX_IO_BYTES);
        let end = start.saturating_add(length).min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }

    fn write_at(&mut self, out: Sink, offset: u64, bytes: &[u8]) -> Result<(), Failure> {
        if bytes.len() > MAX_IO_BYTES {
            return Err(Failure::coded(FailureKind::Io, "plugin_write_too_large"));
        }
        let sink = self.sinks.get_mut(&out).ok_or_else(refused)?;
        let start = usize::try_from(offset)
            .map_err(|_| Failure::coded(FailureKind::Io, "plugin_output_too_large"))?;
        let end = start.saturating_add(bytes.len());
        if sink.len() < end {
            sink.resize(end, 0);
        }
        sink[start..end].copy_from_slice(bytes);
        Ok(())
    }

    fn should_stop(&mut self) -> bool {
        self.stop
    }

    fn setting(&mut self, name: &str) -> Option<String> {
        self.settings.get(name).cloned()
    }
}
