//! One opened document: a store that lives as long as the document is shown, its limits applied
//! per call (`docs/plugins.md`, "Long-lived stores"), and every answer checked before the window
//! sees it.
//!
//! New in rCommander: rDownloader runs one store per call (`component.rs` at commit 26c8592a6);
//! a viewer pages through a document, so its store stays.

use std::sync::{Arc, atomic::AtomicBool};

use rc_plugin_api::viewer::{Document, Output, Token};
use rc_plugin_types::{Failure, FailureKind};
use wasmtime::Store;

use super::call::{Ended, Shared, bounded, settle};
use super::guest::ViewerGuest;
use super::tree::{TreeChildren, TreeState};
use super::validate::{self, ImageFormat, MAX_TOKEN_LINES};
use crate::{
    CallSlot, Invocation,
    bindings::rcommander::plugin::types,
    failure::{self, codes},
    runtime::PluginStoreState,
    streams::SinkWriter,
};

/// Longest needle `search` hands a plugin.
pub const MAX_NEEDLE_CHARS: usize = 1024;

/// What `text` wrote.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextOutput {
    /// The encoding, lower-cased (`utf-8`).
    pub encoding: String,
    /// The bytes that reached the sink.
    pub bytes: u64,
}

/// What `image` wrote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageOutput {
    pub format: ImageFormat,
    pub bytes: u64,
}

/// A document a viewer plugin opened.
///
/// Every method is one call into the plugin with the manifest's fuel and time; a call that
/// traps, runs out or is cancelled closes the session (`plugin_session_closed` afterwards) and the
/// document is opened again. Outputs the document did not name are refused before the plugin
/// runs.
pub struct ViewerSession {
    pub(crate) shared: Arc<Shared>,
    pub(crate) store: Store<PluginStoreState>,
    pub(crate) guest: ViewerGuest,
    pub(crate) document: Document,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) tree: TreeState,
    pub(crate) closed: bool,
}

impl ViewerSession {
    /// What `open` answered, checked.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The flag that stops this session's running call and every later one: `should-stop`
    /// answers it, and a call still running [`crate::CANCEL_GRACE`] later is ended.
    #[must_use]
    pub fn cancellation(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    /// Whether an earlier call stopped the instance; the document has to be opened again.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Writes the document as text into `sink` (for `text`, `highlighted` and `markdown`).
    pub async fn text(&mut self, sink: Box<dyn SinkWriter>) -> Result<TextOutput, Failure> {
        let offered = self.offers(Output::is_textual);
        let (invocation, _running) = self.start(offered, "text").await?;
        let out = self.add_sink(sink);
        let wall = self.shared.wall_clock();
        let ended = bounded(wall, self.guest.call_text(&mut self.store, out)).await;
        let answered = self.settle(ended);
        let result = match self.close_sink(out, answered.is_ok()).await {
            Ok(bytes) => answered.and_then(|encoding| {
                validate::encoding(&encoding).map(|encoding| TextOutput { encoding, bytes })
            }),
            Err(failure) => answered.and(Err(failure)),
        };
        self.end(invocation, result)
    }

    /// The token spans of the lines on screen, `lines[0]` being line `first_line` (for
    /// `highlighted`): one entry per line, `None` for a line shown uncoloured.
    pub async fn tokens(
        &mut self,
        first_line: u32,
        lines: &[&str],
    ) -> Result<Vec<Option<Vec<Token>>>, Failure> {
        let count = u32::try_from(lines.len())
            .ok()
            .filter(|_| lines.len() <= MAX_TOKEN_LINES)
            .ok_or_else(request_too_large)?;
        let offered = self.offers(|output| output == Output::Highlighted);
        let (invocation, _running) = self.start(offered, "tokens").await?;
        let wall = self.shared.wall_clock();
        let ended = bounded(
            wall,
            self.guest.call_tokens(&mut self.store, first_line, count),
        )
        .await;
        let result = self
            .settle(ended)
            .map(|answered| validate::tokens(lines, answered));
        self.end(invocation, result)
    }

    /// Writes the picture into `sink` (for `image`).
    pub async fn image(&mut self, sink: Box<dyn SinkWriter>) -> Result<ImageOutput, Failure> {
        let offered = self.offers(|output| output == Output::Image);
        let (invocation, _running) = self.start(offered, "image").await?;
        let out = self.add_sink(sink);
        let wall = self.shared.wall_clock();
        let ended = bounded(wall, self.guest.call_image(&mut self.store, out)).await;
        let answered = self.settle(ended);
        let result = match self.close_sink(out, answered.is_ok()).await {
            Ok(bytes) => answered.and_then(|format| {
                validate::image(&format, bytes).map(|format| ImageOutput { format, bytes })
            }),
            Err(failure) => answered.and(Err(failure)),
        };
        self.end(invocation, result)
    }

    /// Renders page `index` as an RGBA raster of `width` x `height` into `sink` (for `pages`);
    /// answers the bytes written, exactly `width * height * 4`.
    pub async fn page(
        &mut self,
        index: u32,
        width: u32,
        height: u32,
        sink: Box<dyn SinkWriter>,
    ) -> Result<u64, Failure> {
        let expected = validate::raster_bytes(width, height)
            .filter(|bytes| *bytes <= self.shared.output_limit)
            .ok_or_else(request_too_large)?;
        if self.document.pages.is_some_and(|pages| index >= pages) {
            return Err(request_too_large());
        }
        let offered = self.offers(|output| output == Output::Pages);
        let (invocation, _running) = self.start(offered, "page").await?;
        let out = self.add_sink(sink);
        let wall = self.shared.wall_clock();
        let ended = bounded(
            wall,
            self.guest
                .call_page(&mut self.store, index, width, height, out),
        )
        .await;
        let answered = self.settle(ended);
        let result = match self.close_sink(out, answered.is_ok()).await {
            Ok(bytes) if answered.is_ok() && bytes != expected => {
                Err(failure::bad_output("raster_length"))
            }
            Ok(bytes) => answered.map(|()| bytes),
            Err(failure) => answered.and(Err(failure)),
        };
        self.end(invocation, result)
    }

    /// The children of `node`, or the roots for `None` (for `tree`). Only nodes the plugin
    /// handed out are expanded.
    pub async fn children(&mut self, node: Option<u64>) -> Result<TreeChildren, Failure> {
        let depth = self.tree.depth_below(node)?;
        let offered = self.offers(|output| output == Output::Tree);
        let (invocation, _running) = self.start(offered, "children").await?;
        let wall = self.shared.wall_clock();
        let ended = bounded(wall, self.guest.call_children(&mut self.store, node)).await;
        let result = self
            .settle(ended)
            .map(|answered| self.tree.accept(depth, answered));
        self.end(invocation, result)
    }

    /// The next page from `from_page` holding `needle` (for `pages`; `text` is searched by
    /// rCommander).
    pub async fn search(
        &mut self,
        needle: &str,
        from_page: u32,
        backwards: bool,
    ) -> Result<Option<u32>, Failure> {
        if needle.is_empty() || needle.chars().count() > MAX_NEEDLE_CHARS {
            return Err(request_too_large());
        }
        let offered = self.offers(|output| output == Output::Pages);
        let (invocation, _running) = self.start(offered, "search").await?;
        let pages = self.document.pages.unwrap_or(0);
        let wall = self.shared.wall_clock();
        let ended = bounded(
            wall,
            self.guest
                .call_search(&mut self.store, needle, from_page, backwards),
        )
        .await;
        let result = self.settle(ended).and_then(|found| match found {
            Some(page) if page >= pages => Err(failure::bad_output("search_page")),
            found => Ok(found),
        });
        self.end(invocation, result)
    }

    fn offers(&self, wanted: impl Fn(Output) -> bool) -> bool {
        self.document.outputs.iter().any(|output| wanted(*output))
    }

    /// The checks before a call, a place among the plugin's `limits.max_concurrent` (held until
    /// the returned slot drops), then its budget. A refusal here runs nothing and is no record.
    async fn start(
        &mut self,
        offered: bool,
        operation: &'static str,
    ) -> Result<(Invocation, CallSlot), Failure> {
        if self.closed {
            return Err(failure::coded(
                FailureKind::Other,
                codes::PLUGIN_SESSION_CLOSED,
            ));
        }
        if !offered {
            return Err(failure::coded(
                FailureKind::NotSupported,
                codes::PLUGIN_OUTPUT_NOT_OFFERED,
            ));
        }
        self.shared.health.check()?;
        let running = self.shared.enter().await;
        self.shared
            .sandbox
            .begin_call(&mut self.store, operation)
            .map_err(failure::host_failed)?;
        Ok((self.shared.log.begin(operation), running))
    }

    fn end<T>(&mut self, invocation: Invocation, result: Result<T, Failure>) -> Result<T, Failure> {
        self.shared.finish(invocation, &result);
        result
    }

    fn settle<T>(&mut self, ended: Ended<Result<T, types::Failure>>) -> Result<T, Failure> {
        let cancelled = self.store.data().cancelled();
        let mut stopped = false;
        let result = settle(ended, cancelled, &mut stopped);
        self.closed |= stopped;
        result
    }

    fn add_sink(&mut self, sink: Box<dyn SinkWriter>) -> u32 {
        let limit = self.shared.output_limit;
        self.store.data_mut().streams.add_sink(sink, limit)
    }

    /// Takes a call's sink back: refused when a write broke a rule (even if the plugin ignored
    /// the error it was given), made durable when the call succeeded. Answers the host's count of
    /// the bytes written.
    async fn close_sink(&mut self, out: u32, succeeded: bool) -> Result<u64, Failure> {
        let Some(mut slot) = self.store.data_mut().streams.take_sink(out) else {
            return Err(failure::host_failed("a call's sink is gone"));
        };
        if let Some(code) = slot.refused {
            return Err(failure::coded(FailureKind::Io, code));
        }
        if succeeded {
            slot.sync().await?;
        }
        Ok(slot.committed)
    }
}

fn request_too_large() -> Failure {
    failure::coded(FailureKind::Other, codes::PLUGIN_REQUEST_TOO_LARGE)
}
