//! A plugin's document in the viewer (RC-100-10, `docs/plugins.md` "Viewer (WLX)"): the views it
//! offers - coloured text and a tree beside the Lister's own text, Markdown and image views - and
//! where its token spans and tree nodes come from. The types mirror the WIT viewer world ("The
//! worlds": `output`, `token-class`, `token`, `tree-node`) without depending on the plugin
//! runtime: RC-100-13 makes a [`Document`] from a plugin session, its [`DocumentSource`] asking the
//! session, and hands it to a Lister or Quick View (`plugin.rs`). The tests use a
//! fake source.
//!
//! The source is asked off the UI thread only: tokens for the lines on screen (`spans.rs`), the
//! children of a node when it unfolds (`tree.rs`), and the output a view needs that is not
//! written yet - a picture beside the text, or the text beside a picture (RC-100-29,
//! `output.rs`). Until an answer arrives the text shows without colour, the node with its
//! loading sign and the picture as loading; the window never waits. Switching views and the
//! answers are `views.rs`; drawing the tree and the view list `tree_view.rs`.

use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use rc_engine::plugins::PLUGIN_OUTPUT_NOT_OFFERED;

use super::{
    lister::{Lister, Request},
    load::{Opened, Picture},
    spans::Spans,
    tree::Tree,
};
use crate::i18n::t;

/// The class of a token: WIT `token-class`. Language-neutral, so JSON and later CSS, PHP or
/// JavaScript plugins share it; the theme gives each its colour (`syntax.*`), never the plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenClass {
    Keyword,
    Key,
    String,
    Number,
    Literal,
    Comment,
    Punctuation,
    Operator,
    TypeName,
    Function,
    Invalid,
}

/// A coloured part of a line: WIT `token`. `start` and `len` are UTF-8 bytes within its line,
/// the line break not counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub start: u32,
    pub len: u32,
    pub class: TokenClass,
}

/// A node of a tree: WIT `tree-node`. `id` is the plugin's, unique within the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub id: u64,
    pub label: String,
    pub value: Option<String>,
    pub has_children: bool,
}

/// A view of a document: the WIT `output`s the viewer draws (`pages` comes later).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocView {
    /// The text without colour.
    Text,
    /// The same text with the token spans coloured.
    Highlighted,
    Markdown,
    Image,
    Tree,
}

impl DocView {
    /// Whether the view shows the document's text: plain, coloured or as Markdown.
    pub fn is_text(self) -> bool {
        matches!(self, Self::Text | Self::Highlighted | Self::Markdown)
    }
}

impl fmt::Display for DocView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Text => t!("viewer.view_text"),
            Self::Highlighted => t!("viewer.view_highlighted"),
            Self::Markdown => t!("viewer.view_markdown"),
            Self::Image => t!("viewer.view_image"),
            Self::Tree => t!("viewer.view_tree"),
        })
    }
}

/// An answer of a source: the value, or a stable error code for the status line.
pub type Answer<T> = Pin<Box<dyn Future<Output = Result<T, &'static str>> + Send>>;

/// Where a document's spans and nodes come from: the plugin session (RC-100-13), a fake in tests.
/// Each call runs as a task; the window drops an answer for a document no longer shown.
pub trait DocumentSource: fmt::Debug + Send + Sync {
    /// The tokens of the lines of the document's text that start at `starts` (byte offsets into
    /// the text the Lister shows, in the order of the page), one list per line; an empty list for
    /// a line without tokens or one the source cannot place.
    fn tokens(&self, starts: Vec<u64>) -> Answer<Vec<Vec<Token>>>;

    /// The children of `node`; the top level for `None`.
    fn children(&self, node: Option<u64>) -> Answer<Vec<TreeNode>>;

    /// The document's text opened for the Lister, written on the first ask (RC-100-29). A
    /// source without text answers that it offers none.
    fn text(&self) -> Answer<Opened> {
        Box::pin(async { Err(PLUGIN_OUTPUT_NOT_OFFERED) })
    }

    /// The document's picture, decoded for the image view and written on the first ask. A
    /// source without a picture answers that it offers none.
    fn picture(&self) -> Answer<Picture> {
        Box::pin(async { Err(PLUGIN_OUTPUT_NOT_OFFERED) })
    }
}

/// Where a document's text stands against the Lister's file (RC-100-29).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFile {
    /// The Lister's file is the text, or the document has none.
    Shown,
    /// Written on the first switch to a text view; the Lister shows the file itself until then.
    Later,
    /// Asked for; its answer has not come.
    Asked,
}

/// The serial of the next document: an answer names the document it was asked for.
static NEXT_SERIAL: AtomicU64 = AtomicU64::new(1);

/// A document in a Lister: its views, the one shown, its source and what it has answered.
#[derive(Debug)]
pub struct Document {
    views: Vec<DocView>,
    current: usize,
    source: Arc<dyn DocumentSource>,
    serial: u64,
    text: TextFile,
    /// `plugins.viewer_token_colors`, as the window last read it.
    pub colors: bool,
    pub spans: Spans,
    pub tree: Tree,
}

impl Document {
    /// A document offering `views`, the first shown; a view named twice counts once. `None`
    /// without any view.
    pub fn new(views: Vec<DocView>, source: Arc<dyn DocumentSource>) -> Option<Self> {
        let mut offered: Vec<DocView> = Vec::with_capacity(views.len());
        for view in views {
            if !offered.contains(&view) {
                offered.push(view);
            }
        }
        if offered.is_empty() {
            return None;
        }
        Some(Self {
            views: offered,
            current: 0,
            source,
            serial: NEXT_SERIAL.fetch_add(1, Ordering::Relaxed),
            text: TextFile::Shown,
            colors: true,
            spans: Spans::default(),
            tree: Tree::default(),
        })
    }

    pub fn views(&self) -> &[DocView] {
        &self.views
    }

    /// The view shown.
    pub fn view(&self) -> DocView {
        self.views[self.current]
    }

    pub fn serial(&self) -> u64 {
        self.serial
    }

    pub fn source(&self) -> Arc<dyn DocumentSource> {
        Arc::clone(&self.source)
    }

    /// The same document, its text written on the first switch to a text view: its first
    /// output is its picture (RC-100-29).
    #[must_use]
    pub fn text_later(mut self) -> Self {
        self.text = TextFile::Later;
        self
    }

    /// Where the text stands against the Lister's file.
    pub fn text_file(&self) -> TextFile {
        self.text
    }

    /// Asks for the text once: `true` the first time.
    pub(super) fn ask_text(&mut self) -> bool {
        let first = self.text == TextFile::Later;
        if first {
            self.text = TextFile::Asked;
        }
        first
    }

    /// The text is the Lister's file now.
    pub(super) fn text_shown(&mut self) {
        self.text = TextFile::Shown;
    }

    /// Whether the document offers `view`.
    pub fn offers(&self, view: DocView) -> bool {
        self.views.contains(&view)
    }

    /// The views `gone` takes away no longer offered - an output that could not be written -
    /// and the first left shown when the one shown went; the last view always stays.
    pub(super) fn drop_views(&mut self, gone: impl Fn(DocView) -> bool) {
        let shown = self.view();
        let kept: Vec<DocView> = self.views.iter().copied().filter(|v| !gone(*v)).collect();
        if kept.is_empty() {
            return;
        }
        self.current = kept.iter().position(|view| *view == shown).unwrap_or(0);
        self.views = kept;
    }

    /// The view after the one shown, the first after the last.
    pub(super) fn next_view(&mut self) -> DocView {
        self.current = (self.current + 1) % self.views.len();
        self.view()
    }

    /// Shows `view`; `false` when the document does not offer it or shows it already.
    pub(super) fn choose(&mut self, view: DocView) -> bool {
        match self.views.iter().position(|offered| *offered == view) {
            Some(at) if at != self.current => {
                self.current = at;
                true
            }
            _ => false,
        }
    }

    /// Leaves the tree for the text a key shows: the coloured text, the plain one, else the next
    /// view.
    pub(super) fn leave_tree(&mut self) {
        let text = [DocView::Highlighted, DocView::Text]
            .into_iter()
            .find_map(|view| self.views.iter().position(|offered| *offered == view));
        self.current = text.unwrap_or((self.current + 1) % self.views.len());
    }
}

/// An answer or an input of a document's views.
#[derive(Debug, Clone)]
pub enum DocumentMessage {
    /// The tokens of the lines at these starts, for the document of this serial.
    Tokens(u64, Vec<u64>, Result<Vec<Vec<Token>>, &'static str>),
    /// The children of a node (the top level for `None`), for the document of this serial.
    Children(u64, Option<u64>, Result<Vec<TreeNode>, &'static str>),
    /// The document's text opened, written on the first switch to it, for the document of this
    /// serial (RC-100-29).
    Text(u64, Result<Opened, &'static str>),
    /// The document's picture decoded, for the document of this serial.
    Picture(u64, Result<Picture, &'static str>),
    /// A view chosen in the list of views.
    View(DocView),
    /// A row of the tree clicked, by its index in the unfolded tree.
    TreeClick(usize),
    /// The wheel over the tree, in rows; above 0 down.
    TreeScroll(i32),
    /// The tree's height measured, in pixels.
    TreeViewport(f32),
}

impl Lister {
    /// `document` shown in this lister: its first view, the requests that view needs.
    pub fn set_document(&mut self, document: Document) -> Vec<Request> {
        let first = document.view();
        self.document = Some(document);
        self.apply_view(first)
    }
}

#[cfg(test)]
impl crate::app::App {
    /// Hands `document` to the Lister or Quick View `id`, as `plugin.rs` does for a plugin's
    /// document: it shows the document's first view, with the colours
    /// `plugins.viewer_token_colors` allows. The tests attach their fake sources with it.
    pub(in crate::app) fn attach_document(
        &mut self,
        id: u64,
        mut document: Document,
    ) -> Vec<crate::app::Effect> {
        document.colors = self.viewers.token_colors.0;
        let Some(lister) = self.viewers.lister_mut(id) else {
            return Vec::new();
        };
        let requests = lister.set_document(document);
        self.run(id, requests)
    }
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;
