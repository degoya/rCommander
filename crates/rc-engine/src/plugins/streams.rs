//! What a viewer plugin reads and writes: the file through the viewer's [`FileWindow`] on any
//! backend (local, archive, SFTP, FTP), window by window; its output into a temporary file of the
//! session that the Lister opens as any file (`docs/plugins.md`, "Viewer (WLX)": `text`).
//!
//! While the plugin writes its text in order, the line starts are counted on the way
//! ([`LineFeed`]), so a page's line-start offsets turn into the line numbers `tokens` asks by
//! without reading the output again; a plugin writing out of order costs one pass afterwards.

use std::{
    io::{self, SeekFrom},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use rc_plugin_api::SourceInfo;
use rc_plugin_host::{SinkWriter, SourceReader};
use tokio::io::{AsyncSeekExt as _, AsyncWriteExt as _};

use crate::viewer::{Detected, FileWindow, LineIndex, TextEncoding};

/// The file a plugin is handed: its name, size and time, and its bytes - never its folder.
pub(crate) struct WindowSource {
    file: Arc<FileWindow>,
    info: SourceInfo,
}

impl WindowSource {
    pub(crate) fn new(file: Arc<FileWindow>, name: &str) -> Self {
        let modified = file
            .modified()
            .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339());
        let info = SourceInfo {
            name: name.to_owned(),
            size: file.size(),
            modified,
        };
        Self { file, info }
    }
}

#[async_trait]
impl SourceReader for WindowSource {
    fn info(&self) -> SourceInfo {
        self.info.clone()
    }

    async fn read_at(&self, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        // Uncached: a plugin reads its file once, in windows larger than the cache's blocks.
        self.file
            .read_uncached(offset, length)
            .await
            .map_err(io::Error::other)
    }
}

/// The line starts of text written in order. Counted as UTF-8: a line break is the same byte in
/// every encoding the viewer reads but UTF-16, whose output is counted again afterwards.
#[derive(Debug)]
pub(crate) struct LineFeed {
    index: Option<LineIndex>,
}

impl Default for LineFeed {
    fn default() -> Self {
        Self {
            index: Some(LineIndex::new(Detected::chosen(TextEncoding::Utf8, &[]))),
        }
    }
}

impl LineFeed {
    /// `bytes` written at `offset`: counted when they continue the text, else the count is
    /// given up.
    fn wrote(&mut self, offset: u64, bytes: &[u8]) {
        let in_order = self
            .index
            .as_ref()
            .is_some_and(|index| index.next_offset() == offset);
        match &mut self.index {
            Some(index) if in_order => index.feed(bytes),
            _ => self.index = None,
        }
    }

    /// The count of `written` bytes in `encoding`, when the feed saw all of them in order and
    /// the encoding breaks lines as UTF-8 does.
    pub(crate) fn finish(self, written: u64, encoding: TextEncoding) -> Option<LineIndex> {
        let ascii_breaks = encoding.unit() == 1;
        let mut index = self
            .index
            .filter(|index| ascii_breaks && index.next_offset() == written)?;
        index.finish();
        Some(index)
    }
}

/// The session's output file, written where the plugin says, its text lines counted.
pub(crate) struct OutputSink {
    file: tokio::fs::File,
    feed: Arc<Mutex<LineFeed>>,
}

impl OutputSink {
    pub(crate) fn new(file: std::fs::File, feed: Arc<Mutex<LineFeed>>) -> Self {
        Self {
            file: tokio::fs::File::from_std(file),
            feed,
        }
    }
}

#[async_trait]
impl SinkWriter for OutputSink {
    async fn write_at(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(offset)).await?;
        self.file.write_all(bytes).await?;
        if let Ok(mut feed) = self.feed.lock() {
            feed.wrote(offset, bytes);
        }
        Ok(())
    }

    async fn sync(&mut self) -> io::Result<()> {
        self.file.flush().await
    }
}

#[cfg(test)]
#[path = "streams_tests.rs"]
mod tests;
