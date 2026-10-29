//! A document a viewer plugin opened, as the window shows it: the output files the Lister reads,
//! the views offered, and the plugin's session behind the questions the views ask - the token
//! spans of the lines on screen, the children of a tree node, and the output not written yet.
//!
//! A document may offer a picture and text (RC-100-29). Each has a file of its own; the first
//! one is written before the window shows the document, the other on the first switch to it -
//! [`PluginDocument::write_text`], [`PluginDocument::write_picture`] - with the session kept
//! open for it as for the tokens and the tree.
//!
//! The window counts lines by their start in the output file; the plugin by their number. The
//! line index of the output turns one into the other, and the lines' own text goes with the
//! question, so the host can check every span against the line it colours.

use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use rc_plugin_api::viewer::{Output, Token, TreeNode};
use rc_plugin_host::{MAX_TOKEN_LINES, ViewerSession, codes, installed::InUse};
use tempfile::TempPath;
use tokio::sync::OnceCell;

use super::{codes::failure_code, write};
use crate::{
    Engine,
    viewer::{Detected, FileWindow, LineIndex, text},
};

/// One more open document while it lives.
#[derive(Debug)]
pub(crate) struct LiveGuard(Arc<AtomicUsize>);

impl LiveGuard {
    pub(crate) fn new(live: &Arc<AtomicUsize>) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self(Arc::clone(live))
    }
}

impl Drop for LiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The text a plugin wrote: the file, how it reads, where its lines start.
pub(crate) struct WrittenText {
    pub(crate) file: Arc<FileWindow>,
    pub(crate) detected: Detected,
    pub(crate) index: LineIndex,
    /// Last, so the window on it is closed before the file is removed.
    pub(crate) path: TempPath,
}

/// How the picture output reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureFormat {
    /// PNG, JPEG, GIF, BMP or WebP: the image view tells which from the file's head.
    Encoded,
    /// Straight RGBA, exactly `width * height * 4` bytes, as `rgba:<w>x<h>` named it.
    Rgba { width: u32, height: u32 },
}

/// The picture a plugin wrote.
pub(crate) struct WrittenPicture {
    pub(crate) format: PictureFormat,
    pub(crate) path: TempPath,
}

/// A plugin's document, open until it is dropped; its files are removed then.
pub struct PluginDocument {
    pub(crate) plugin: String,
    pub(crate) outputs: Vec<Output>,
    pub(crate) title: Option<String>,
    /// The text (`text`, `highlighted`, `markdown`), once written.
    pub(crate) text: OnceCell<WrittenText>,
    /// The picture (`image`), once written.
    pub(crate) picture: OnceCell<WrittenPicture>,
    pub(crate) session: tokio::sync::Mutex<ViewerSession>,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) _live: LiveGuard,
    /// The plugin's version folder, not removed while the document is open.
    pub(crate) _in_use: Option<InUse>,
}

impl std::fmt::Debug for PluginDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginDocument")
            .field("plugin", &self.plugin)
            .field("outputs", &self.outputs)
            .finish_non_exhaustive()
    }
}

impl PluginDocument {
    /// The name of the plugin that opened it.
    #[must_use]
    pub fn plugin(&self) -> &str {
        &self.plugin
    }

    /// The outputs the window can show, the first shown first.
    #[must_use]
    pub fn outputs(&self) -> &[Output] {
        &self.outputs
    }

    /// The title the plugin gave the document, checked by the host.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// The file of the text output, once written: the Lister opens it in place of the original.
    #[must_use]
    pub fn text_path(&self) -> Option<&Path> {
        self.text.get().map(|text| &*text.path)
    }

    /// How the text output reads: the encoding the plugin named.
    #[must_use]
    pub fn detected(&self) -> Option<Detected> {
        self.text.get().map(|text| text.detected)
    }

    /// The file of the picture output and how it reads, once written.
    #[must_use]
    pub fn picture(&self) -> Option<(&Path, PictureFormat)> {
        self.picture
            .get()
            .map(|picture| (&*picture.path, picture.format))
    }

    /// The text output, written on the first ask: its file and how it reads. A document whose
    /// first output is its picture writes its text when the person first switches to it.
    ///
    /// # Errors
    ///
    /// The document offers no text, the person moved on, or the plugin failed: a stable code.
    /// The session may be closed then; what is written stays readable.
    pub async fn write_text(&self, engine: &Engine) -> Result<(&Path, Detected), &'static str> {
        if !self.outputs.iter().any(|output| output.is_textual()) {
            return Err(codes::PLUGIN_OUTPUT_NOT_OFFERED);
        }
        let written = self
            .text
            .get_or_try_init(|| async {
                if self.is_cancelled() {
                    return Err(codes::PLUGIN_CANCELLED);
                }
                let mut session = self.session.lock().await;
                write::text(engine, &mut session, &self.cancel)
                    .await
                    .map_err(write::Stop::code)
            })
            .await?;
        Ok((&*written.path, written.detected))
    }

    /// The picture output, written on the first ask: its file and how it reads.
    ///
    /// # Errors
    ///
    /// As [`PluginDocument::write_text`], for the picture.
    pub async fn write_picture(&self) -> Result<(&Path, PictureFormat), &'static str> {
        if !self.outputs.contains(&Output::Image) {
            return Err(codes::PLUGIN_OUTPUT_NOT_OFFERED);
        }
        let written = self
            .picture
            .get_or_try_init(|| async {
                if self.is_cancelled() {
                    return Err(codes::PLUGIN_CANCELLED);
                }
                let mut session = self.session.lock().await;
                write::picture(&mut session)
                    .await
                    .map_err(write::Stop::code)
            })
            .await?;
        Ok((&*written.path, written.format))
    }

    /// Whether the person moved on: every later call ends at once.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// The token spans of the lines of the text output that start at `starts`, one list per
    /// line, in order; an empty list for a line the plugin leaves uncoloured or the index cannot
    /// place. Lines in a row are asked in one call of at most [`MAX_TOKEN_LINES`].
    ///
    /// # Errors
    ///
    /// The output could not be read, or the plugin failed: a stable code.
    pub async fn tokens(&self, starts: &[u64]) -> Result<Vec<Vec<Token>>, &'static str> {
        let mut answer = vec![Vec::new(); starts.len()];
        let Some(written) = self.text.get() else {
            return Ok(answer);
        };
        let mut at = 0;
        while at < starts.len() {
            let count = (starts.len() - at).min(MAX_TOKEN_LINES);
            let page = text::read_lines(&written.file, written.detected, starts[at], count)
                .await
                .map_err(|error| error.code())?;
            let run = page
                .lines
                .iter()
                .zip(&starts[at..])
                .take_while(|(line, start)| line.offset == **start)
                .count();
            let first = written
                .index
                .line_at(&written.file, starts[at])
                .await
                .map_err(|error| error.code())?
                .and_then(|line| u32::try_from(line).ok());
            let Some(first) = first.filter(|_| run > 0) else {
                at += 1;
                continue;
            };
            let lines: Vec<&str> = page.lines[..run]
                .iter()
                .map(|line| line.text.as_str())
                .collect();
            let spans = self
                .session
                .lock()
                .await
                .tokens(first, &lines)
                .await
                .map_err(|failure| failure_code(failure.code()))?;
            for (slot, spans) in answer[at..at + run].iter_mut().zip(spans) {
                *slot = spans.unwrap_or_default();
            }
            at += run;
        }
        Ok(answer)
    }

    /// The children of `node`, the top level for `None`; past the host's caps the rest is
    /// dropped.
    ///
    /// # Errors
    ///
    /// The plugin failed, or `node` is not one it handed out: a stable code.
    pub async fn children(&self, node: Option<u64>) -> Result<Vec<TreeNode>, &'static str> {
        self.session
            .lock()
            .await
            .children(node)
            .await
            .map(|children| children.nodes)
            .map_err(|failure| failure_code(failure.code()))
    }
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;
