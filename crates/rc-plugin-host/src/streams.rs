//! Sources and sinks: the only bytes a plugin reaches, addressed by handle, never by path.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/extension/source.rs`
//! (`SourceState`, `read_at`, the fuel credit `FUEL_PER_READ_BYTE` and `credited_passes`,
//! RD-191-06, PLUG-01) and `src/transfer/sink.rs` (the host's high-water mark `committed`) at
//! commit 26c8592a6. rDownloader's source is a package folder on disk; here it is whatever file
//! rCommander opened on whatever backend ([`SourceReader`]), and a sink is a target rCommander
//! opened ([`SinkWriter`]) with a limit the host keeps (`docs/plugins.md`, "Bytes through
//! host-provided streams"). The credit is paid on writes too.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use rc_plugin_api::SourceInfo;
use rc_plugin_types::{Failure, FailureKind};

use crate::failure::codes;

/// Largest single read or write: one window.
pub const MAX_WINDOW_BYTES: u32 = 1024 * 1024;

/// Fuel a read credits for every byte it hands the guest, and a write for every byte it takes
/// (rDownloader's value, rounded up from SHA-256's 77 fuel a byte).
pub(crate) const FUEL_PER_BYTE: u64 = 128;

/// How many times over its sources and sinks one call is credited: reading a file twice, a sink
/// written twice. Past that the manifest's own budget runs out as always, so a loop over the
/// same window cannot earn itself an endless budget.
pub(crate) const CREDITED_PASSES: u64 = 2;

/// A file rCommander opened for a plugin: local, in an archive, on a server.
#[async_trait]
pub trait SourceReader: Send + Sync {
    /// Its name, size and time - never its folder.
    fn info(&self) -> SourceInfo;
    /// Up to `length` bytes from `offset`; fewer at the end of the file.
    async fn read_at(&self, offset: u64, length: usize) -> std::io::Result<Vec<u8>>;
}

/// A target rCommander opened for one call's output: a temporary file of the session, a pipe.
#[async_trait]
pub trait SinkWriter: Send {
    async fn write_at(&mut self, offset: u64, bytes: &[u8]) -> std::io::Result<()>;
    /// Makes what was written durable; called by the host once the call succeeded.
    async fn sync(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A source held in memory.
#[derive(Clone, Debug)]
pub struct BytesSource {
    info: SourceInfo,
    bytes: Arc<[u8]>,
}

impl BytesSource {
    #[must_use]
    pub fn new(name: &str, bytes: impl Into<Arc<[u8]>>) -> Self {
        let bytes = bytes.into();
        Self {
            info: SourceInfo {
                name: name.to_owned(),
                size: bytes.len() as u64,
                modified: None,
            },
            bytes,
        }
    }
}

#[async_trait]
impl SourceReader for BytesSource {
    fn info(&self) -> SourceInfo {
        self.info.clone()
    }

    async fn read_at(&self, offset: u64, length: usize) -> std::io::Result<Vec<u8>> {
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.bytes.len());
        let end = start.saturating_add(length).min(self.bytes.len());
        Ok(self.bytes[start..end].to_vec())
    }
}

/// A sink held in memory; [`MemorySink::contents`] shares what was written.
#[derive(Clone, Debug, Default)]
pub struct MemorySink {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl MemorySink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What was written so far.
    #[must_use]
    pub fn contents(&self) -> Vec<u8> {
        self.bytes
            .lock()
            .map(|bytes| bytes.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl SinkWriter for MemorySink {
    async fn write_at(&mut self, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
        let mut held = self
            .bytes
            .lock()
            .map_err(|_| std::io::Error::other("memory sink poisoned"))?;
        let start = usize::try_from(offset).map_err(std::io::Error::other)?;
        let end = start.saturating_add(bytes.len());
        if held.len() < end {
            held.resize(end, 0);
        }
        held[start..end].copy_from_slice(bytes);
        Ok(())
    }
}

/// One sink of a call, with what the host knows about it.
pub(crate) struct SinkSlot {
    writer: Box<dyn SinkWriter>,
    limit: u64,
    /// The high-water mark of what reached the writer: the host's figure, not the plugin's.
    pub(crate) committed: u64,
    /// The first rule a write broke; the call's output is refused for it even when the plugin
    /// ignored the error it was given.
    pub(crate) refused: Option<&'static str>,
}

impl SinkSlot {
    /// Makes the output durable once the call succeeded.
    pub(crate) async fn sync(&mut self) -> Result<(), Failure> {
        self.writer
            .sync()
            .await
            .map_err(|error| sink_failed(&error))
    }
}

/// The sources of a store and the sinks of the call running in it.
#[derive(Default)]
pub(crate) struct Streams {
    sources: BTreeMap<u32, Arc<dyn SourceReader>>,
    sinks: BTreeMap<u32, SinkSlot>,
    /// Handles are never reused within a store, so a stale one is refused, not another stream.
    next_handle: u32,
    read_credit: u64,
    write_credit: u64,
}

impl Streams {
    /// Offers `source` for the life of the store.
    pub(crate) fn add_source(&mut self, source: Arc<dyn SourceReader>) -> u32 {
        let handle = self.next();
        self.read_credit = self.read_credit.saturating_add(
            source
                .info()
                .size
                .saturating_mul(FUEL_PER_BYTE)
                .saturating_mul(CREDITED_PASSES),
        );
        self.sources.insert(handle, source);
        handle
    }

    /// Opens `writer` for the call at hand, holding it to `limit` bytes.
    pub(crate) fn add_sink(&mut self, writer: Box<dyn SinkWriter>, limit: u64) -> u32 {
        let handle = self.next();
        self.write_credit = self.write_credit.saturating_add(
            limit
                .saturating_mul(FUEL_PER_BYTE)
                .saturating_mul(CREDITED_PASSES),
        );
        self.sinks.insert(
            handle,
            SinkSlot {
                writer,
                limit,
                committed: 0,
                refused: None,
            },
        );
        handle
    }

    /// Takes a call's sink back once the call ended.
    pub(crate) fn take_sink(&mut self, handle: u32) -> Option<SinkSlot> {
        self.sinks.remove(&handle)
    }

    /// Starts a call: no sink of the last one stays, and the credit is paid anew.
    pub(crate) fn begin_call(&mut self) {
        self.sinks.clear();
        self.write_credit = 0;
        self.read_credit = self.sources.values().fold(0_u64, |credit, source| {
            credit.saturating_add(
                source
                    .info()
                    .size
                    .saturating_mul(FUEL_PER_BYTE)
                    .saturating_mul(CREDITED_PASSES),
            )
        });
    }

    fn next(&mut self) -> u32 {
        self.next_handle = self.next_handle.saturating_add(1);
        self.next_handle
    }

    pub(crate) fn info(&self, handle: u32) -> Option<SourceInfo> {
        self.sources.get(&handle).map(|source| source.info())
    }

    /// The source behind `handle`; read with [`read_window`], which holds no borrow of the table
    /// while it waits.
    pub(crate) fn source(&self, handle: u32) -> Result<Arc<dyn SourceReader>, Failure> {
        self.sources.get(&handle).cloned().ok_or_else(unknown)
    }

    /// One window into a sink, within its limit.
    pub(crate) async fn write_at(
        &mut self,
        handle: u32,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), Failure> {
        let slot = self.sinks.get_mut(&handle).ok_or_else(unknown)?;
        let length = bytes.len() as u64;
        let refusal = if length > u64::from(MAX_WINDOW_BYTES) {
            Some(codes::PLUGIN_WRITE_TOO_LARGE)
        } else if offset.saturating_add(length) > slot.limit {
            Some(codes::PLUGIN_OUTPUT_TOO_LARGE)
        } else {
            None
        };
        if let Some(code) = refusal {
            slot.refused.get_or_insert(code);
            return Err(Failure::coded(FailureKind::Io, code));
        }
        if let Err(error) = slot.writer.write_at(offset, bytes).await {
            slot.refused.get_or_insert(codes::PLUGIN_SINK_FAILED);
            return Err(sink_failed(&error));
        }
        slot.committed = slot.committed.max(offset + length);
        Ok(())
    }

    pub(crate) fn committed(&self, handle: u32) -> u64 {
        self.sinks.get(&handle).map_or(0, |slot| slot.committed)
    }

    pub(crate) async fn sync(&mut self, handle: u32) -> Result<(), Failure> {
        self.sinks
            .get_mut(&handle)
            .ok_or_else(unknown)?
            .sync()
            .await
    }

    /// The fuel a read of `bytes` earns, within what is left of this call's credit.
    pub(crate) fn take_read_credit(&mut self, bytes: usize) -> u64 {
        take(&mut self.read_credit, bytes)
    }

    /// The fuel a write of `bytes` earns, within what is left of this call's credit.
    pub(crate) fn take_write_credit(&mut self, bytes: usize) -> u64 {
        take(&mut self.write_credit, bytes)
    }
}

/// One window of a source: at most [`MAX_WINDOW_BYTES`].
pub(crate) async fn read_window(
    source: Arc<dyn SourceReader>,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, Failure> {
    let length = length.min(MAX_WINDOW_BYTES) as usize;
    source.read_at(offset, length).await.map_err(|error| {
        tracing::debug!(%error, "a plugin's source could not be read");
        Failure::coded(FailureKind::Io, codes::PLUGIN_SOURCE_FAILED)
    })
}

fn take(left: &mut u64, bytes: usize) -> u64 {
    let wanted = (bytes as u64).saturating_mul(FUEL_PER_BYTE);
    let credit = wanted.min(*left);
    *left -= credit;
    credit
}

fn unknown() -> Failure {
    Failure::coded(FailureKind::NotFound, codes::PLUGIN_UNKNOWN_HANDLE)
}

fn sink_failed(error: &std::io::Error) -> Failure {
    tracing::debug!(%error, "a plugin's sink could not be written");
    Failure::coded(FailureKind::Io, codes::PLUGIN_SINK_FAILED)
}

#[cfg(test)]
#[path = "streams_tests.rs"]
mod tests;
