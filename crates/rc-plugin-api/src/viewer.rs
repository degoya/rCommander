//! The native side of the `viewer-plugin` world (Lister, WLX): the records of `interface viewer`
//! and the [`Viewer`] trait a viewer's logic implements against a [`ViewerHost`].
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-api/src/lib.rs` at commit 26c8592a6
//! (the native `Resolver` trait beside its WIT world). Synchronous, unlike rDownloader's async
//! resolver: a component is single-threaded and its exports are plain calls, and the host owns
//! everything that waits. The host evaluates the manifest's `[match]` itself, so a viewer runs
//! only for files it claimed (`docs/plugins.md`, "Viewer (WLX)").

use serde::{Deserialize, Serialize};

use crate::{Failure, FailureKind, Sink, Source, SourceInfo};

/// Why the document is opened: the Lister window (F3) or the Quick View panel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Purpose {
    Lister,
    QuickView,
}

/// One way a document can be shown. `open` names every one it offers, the first shown; the
/// manifest's `[viewer] outputs` lists the ones the plugin may offer at all.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Output {
    /// Text into a sink, shown by the built-in text viewer.
    Text,
    /// Text into a sink plus token spans per line, coloured by the theme.
    Highlighted,
    /// CommonMark into a sink, shown by the Markdown preview.
    Markdown,
    /// An encoded picture or a raw RGBA raster into a sink.
    Image,
    /// Nodes fetched on expansion.
    Tree,
    /// A paged document rendered at the size rCommander draws it.
    Pages,
}

impl Output {
    /// Every output, in the order of the WIT enum.
    pub const ALL: [Self; 6] = [
        Self::Text,
        Self::Highlighted,
        Self::Markdown,
        Self::Image,
        Self::Tree,
        Self::Pages,
    ];

    /// The name the manifest and the WIT contract use.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Highlighted => "highlighted",
            Self::Markdown => "markdown",
            Self::Image => "image",
            Self::Tree => "tree",
            Self::Pages => "pages",
        }
    }

    /// Whether `text` serves this output: everything written as text into a sink.
    #[must_use]
    pub const fn is_textual(self) -> bool {
        matches!(self, Self::Text | Self::Highlighted | Self::Markdown)
    }
}

/// The colours the document is shown in, so a page can be rendered to match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Theme {
    pub dark: bool,
    /// `#rrggbb`.
    pub background: String,
    pub foreground: String,
    pub accent: String,
}

/// What `open` answers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    /// Every output the document offers, the first shown.
    pub outputs: Vec<Output>,
    pub title: Option<String>,
    /// The page count, for `pages`.
    pub pages: Option<u32>,
}

/// One node of a `tree` output; its children are asked for when it is expanded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeNode {
    pub id: u64,
    pub label: String,
    pub value: Option<String>,
    pub has_children: bool,
}

/// What a span of `highlighted` text is; the theme decides its colour, never the plugin.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

/// A coloured span: UTF-8 bytes within its line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token {
    pub start: u32,
    pub len: u32,
    pub class: TokenClass,
}

/// What a viewer may ask of its host: the `host`, `source`, `sink` and `settings` interfaces of
/// its world.
pub trait ViewerHost {
    /// Writes a line to rCommander's log; the host caps and masks it.
    fn log(&mut self, level: &str, message: &str);
    /// The name, size and time of a source.
    fn info(&mut self, src: Source) -> SourceInfo;
    /// Up to `length` bytes (at most 1 MiB) from `offset`; fewer at the end.
    fn read_at(&mut self, src: Source, offset: u64, length: u32) -> Result<Vec<u8>, Failure>;
    /// Writes up to 1 MiB at `offset` of a sink.
    fn write_at(&mut self, out: Sink, offset: u64, bytes: &[u8]) -> Result<(), Failure>;
    /// Whether the person moved on and the call should end early.
    fn should_stop(&mut self) -> bool;
    /// The value of the manifest's setting `name` (`settings.setting`, contract 0.2.0): what the
    /// person chose, else the manifest's default; `None` for a name the manifest does not
    /// declare. A host of a 0.1.0 plugin has none.
    fn setting(&mut self, name: &str) -> Option<String> {
        let _ = name;
        None
    }
}

/// The logic of a viewer plugin: `interface viewer`, one method per export.
///
/// A store lives for one opened document, so `self` keeps what `open` learned for the calls
/// after it. Every output but `open` has a default that answers `not-supported`; a viewer
/// implements the ones its manifest's `[viewer] outputs` lists.
pub trait Viewer {
    /// Whether this file is one the viewer shows, beyond what `[match]` already said.
    fn probe(&mut self, host: &mut dyn ViewerHost, src: Source) -> bool;

    /// Opens a document and names its outputs.
    fn open(
        &mut self,
        host: &mut dyn ViewerHost,
        src: Source,
        purpose: Purpose,
        theme: &Theme,
    ) -> Result<Document, Failure>;

    /// Writes the document as text into `out` and names the encoding written.
    fn text(&mut self, host: &mut dyn ViewerHost, out: Sink) -> Result<String, Failure> {
        let _ = (host, out);
        Err(not_supported())
    }

    /// The token spans of `count` lines from `first_line`, one list per line.
    fn tokens(
        &mut self,
        host: &mut dyn ViewerHost,
        first_line: u32,
        count: u32,
    ) -> Result<Vec<Vec<Token>>, Failure> {
        let _ = (host, first_line, count);
        Err(not_supported())
    }

    /// Writes a picture into `out` and names its format (`png`, ..., `rgba:<w>x<h>`).
    fn image(&mut self, host: &mut dyn ViewerHost, out: Sink) -> Result<String, Failure> {
        let _ = (host, out);
        Err(not_supported())
    }

    /// Renders page `index` as an RGBA raster of `width` x `height` into `out`.
    fn page(
        &mut self,
        host: &mut dyn ViewerHost,
        index: u32,
        width: u32,
        height: u32,
        out: Sink,
    ) -> Result<(), Failure> {
        let _ = (host, index, width, height, out);
        Err(not_supported())
    }

    /// The children of `node`, or the roots for `None`.
    fn children(
        &mut self,
        host: &mut dyn ViewerHost,
        node: Option<u64>,
    ) -> Result<Vec<TreeNode>, Failure> {
        let _ = (host, node);
        Err(not_supported())
    }

    /// The next page from `from_page` holding `needle`.
    fn search(
        &mut self,
        host: &mut dyn ViewerHost,
        needle: &str,
        from_page: u32,
        backwards: bool,
    ) -> Result<Option<u32>, Failure> {
        let _ = (host, needle, from_page, backwards);
        Err(not_supported())
    }

    /// A small picture of `src` into `out`, for a later thumbnail view.
    fn thumbnail(
        &mut self,
        host: &mut dyn ViewerHost,
        src: Source,
        size: u32,
        out: Sink,
    ) -> Result<String, Failure> {
        let _ = (host, src, size, out);
        Err(not_supported())
    }
}

/// The answer of an output a viewer does not offer.
#[must_use]
pub fn not_supported() -> Failure {
    Failure::new(FailureKind::NotSupported)
}

#[cfg(test)]
#[path = "viewer_tests.rs"]
mod tests;
