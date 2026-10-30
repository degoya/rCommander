//! One file in the viewer: what is shown of it and how, without a window.
//!
//! The view keeps its place as the byte offset of its first line or row (`top`); every move is
//! a [`Request`] for the window to run ([`super::load`]) and its answer comes back as a page.
//! A page is only what fits, read through the engine's block cache, so a 5 GB log scrolls as a
//! 5 KB one does. The page's text sits in a read-only `text_editor`, which gives selecting with
//! the mouse, Ctrl+A and Ctrl+C for free; it never edits, and its scrolling is the viewer's.
//! The search and "go to offset" bar is `find.rs`; a Markdown file's preview `markdown.rs`; a
//! plugin's document with its views `document.rs` (RC-100-10).

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use iced::widget::text_editor::{Action, Content, Cursor, Position};
use rc_engine::{
    PanelId, VfsPath,
    viewer::{Detected, Direction, Match, Needle, TextEncoding, ViewMode},
};

use super::{
    document::Document,
    keys::{ListerKey, Scroll},
    load::{Layout, Opened, Page, Picture, Seek},
    markdown::{self, Markdown},
};
use crate::i18n::{self, t};

/// The tab widths T steps through; 8 is Lister's.
const TAB_WIDTHS: [usize; 3] = [8, 4, 2];
/// Columns ArrowLeft and ArrowRight move the text without wrap.
const SIDE_STEP: usize = 8;
/// Zoom steps of the image view.
const ZOOM_STEP: f32 = 1.25;
const ZOOM_MIN: f32 = 0.05;
const ZOOM_MAX: f32 = 16.0;

/// What a lister needs read; the window runs it and hands the answer back.
#[derive(Debug, Clone)]
pub enum Request {
    /// A page; an answer to an older request than the latest is dropped.
    Page {
        generation: u64,
        seek: Seek,
        mark: Option<Match>,
    },
    /// The picture; like a page, an answer to an older request than the latest is dropped.
    Picture { generation: u64 },
    Search {
        needle: Needle,
        from: u64,
        direction: Direction,
        cancel: Arc<AtomicBool>,
    },
    /// The file opened again (F2).
    Reload,
    /// A Markdown file read and parsed for its preview; `forced` by the key, whatever the
    /// setting says.
    Render { forced: bool },
    /// The preview scrolled.
    PreviewScroll(Scroll),
    /// The preview's selection copied.
    Copy(String),
    /// A document's tokens for the lines starting at these offsets (RC-100-10).
    Tokens(Vec<u64>),
    /// A document's tree nodes: the children of a node, the top level for `None`.
    Children(Option<u64>),
    /// A document's text, written by its plugin on the first switch to it (RC-100-29).
    Text,
}

/// The picture of the image view.
#[derive(Debug, Clone, Default)]
pub enum Shown {
    #[default]
    None,
    Loading,
    Ready(Picture),
    Failed(&'static str),
}

/// How the picture is shown: fit to the window, or at a scale; turned by quarters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zoom {
    pub fit: bool,
    pub scale: f32,
    pub turns: u8,
}

impl Default for Zoom {
    fn default() -> Self {
        Self {
            fit: true,
            scale: 1.0,
            turns: 0,
        }
    }
}

#[derive(Debug)]
pub struct Lister {
    pub id: u64,
    /// The panel the file was opened from: N and P step through its files.
    pub panel: PanelId,
    pub path: VfsPath,
    pub file: Option<Opened>,
    /// The encoding in use: detected, or chosen with A, S, 6, 8 or E.
    pub detected: Detected,
    pub mode: ViewMode,
    pub wrap: bool,
    pub tab_width: usize,
    pub left: usize,
    /// Lines or rows that fit; measured by the window.
    pub rows: usize,
    pub top: u64,
    pub end: u64,
    pub content: Content,
    pub picture: Shown,
    pub zoom: Zoom,
    pub find: super::find::Find,
    /// The last error or news, for the status line.
    pub note: Option<String>,
    /// A Markdown file's preview; `None` for any other file.
    pub markdown: Option<Markdown>,
    /// The text size of this window in points; `None` for the settings' (RC-020-12).
    pub text_size: Option<f32>,
    /// How far down the preview is scrolled, as a share: a new text size keeps it.
    pub preview_at: f32,
    /// A plugin's document with several views; `None` for a file the Lister shows itself.
    pub document: Option<Document>,
    /// Raised when the lister goes: its plugin's running call and every later one end
    /// (RC-100-13).
    plugin_stop: Arc<AtomicBool>,
    generation: u64,
    /// The latest picture asked for: a reload asks again, and the older answer is dropped.
    picture_generation: u64,
}

impl Drop for Lister {
    /// The lister goes: its plugin's calls end, and so does a search still running through the
    /// file (RC-080-44).
    fn drop(&mut self) {
        self.plugin_stop.store(true, Ordering::SeqCst);
        if let Some(cancel) = &self.find.running {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl Lister {
    pub fn new(id: u64, panel: PanelId, path: VfsPath) -> Self {
        Self {
            id,
            panel,
            file: None,
            detected: rc_engine::viewer::detect(b""),
            mode: ViewMode::Text,
            wrap: true,
            tab_width: TAB_WIDTHS[0],
            left: 0,
            rows: 40,
            top: 0,
            end: 0,
            content: Content::new(),
            picture: Shown::None,
            zoom: Zoom::default(),
            find: super::find::Find::default(),
            note: None,
            markdown: markdown::is_markdown(&path).then(Markdown::default),
            text_size: None,
            preview_at: 0.0,
            document: None,
            plugin_stop: Arc::new(AtomicBool::new(false)),
            path,
            generation: 0,
            picture_generation: 0,
        }
    }

    /// The flag its plugin's calls stop at.
    pub fn plugin_stop(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.plugin_stop)
    }

    /// The file's name, for the title.
    pub fn name(&self) -> String {
        self.path.path.file_name().map_or_else(
            || self.path.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    pub fn size(&self) -> u64 {
        self.file.as_ref().map_or(0, |file| file.file.size())
    }

    pub fn layout(&self) -> Layout {
        Layout {
            mode: self.mode,
            detected: self.detected,
            tab_width: self.tab_width,
            left: if self.wrap { 0 } else { self.left },
            rows: self.rows,
        }
    }

    /// The file is open: its first page, or its picture. DOS text opens unwrapped: its art is
    /// drawn for 80 columns and breaks apart when wrapped.
    pub fn opened(&mut self, opened: Opened) -> Vec<Request> {
        self.detected = opened.detected;
        self.mode = opened.mode;
        if opened.dos {
            self.wrap = false;
        }
        self.file = Some(opened);
        let mut requests = self.show(Seek::At(0), None);
        requests.extend(self.ask_preview());
        requests
    }

    /// The file opened again: the same place, mode and encoding, the new size.
    pub fn reloaded(&mut self, opened: Opened) -> Vec<Request> {
        let chosen = self.detected.encoding;
        self.detected = Detected::chosen(chosen, &opened.head);
        self.file = Some(opened);
        self.picture = Shown::None;
        self.note = Some(t!("viewer.reloaded").to_owned());
        if let Some(document) = &mut self.document {
            document.spans.clear();
        }
        let mut requests = self.show(Seek::At(self.top), None);
        requests.extend(self.preview_reloaded());
        requests
    }

    /// An error of opening, reading or decoding, for the status line.
    pub fn failed(&mut self, code: &str) {
        self.note = Some(i18n::error_text(code));
    }

    /// What `seek` asks of the current mode: a page, or the picture once.
    pub fn show(&mut self, seek: Seek, mark: Option<Match>) -> Vec<Request> {
        if self.file.is_none() {
            return Vec::new();
        }
        if self.mode == ViewMode::Image {
            if matches!(self.picture, Shown::None) {
                self.picture = Shown::Loading;
                self.picture_generation += 1;
                return vec![Request::Picture {
                    generation: self.picture_generation,
                }];
            }
            return Vec::new();
        }
        self.generation += 1;
        vec![Request::Page {
            generation: self.generation,
            seek,
            mark,
        }]
    }

    /// A page arrived; one for an older request than the latest is dropped. A document's
    /// coloured text asks for the tokens of the page's lines.
    pub fn paged(&mut self, generation: u64, page: Result<Page, &'static str>) -> Vec<Request> {
        if generation != self.generation {
            return Vec::new();
        }
        match page {
            Ok(page) => {
                self.top = page.top;
                self.end = page.end;
                self.content = Content::with_text(&page.lines.join("\n"));
                if let Some(span) = page.mark {
                    self.content.move_to(Cursor {
                        position: Position {
                            line: span.to.0,
                            column: span.to.1,
                        },
                        selection: Some(Position {
                            line: span.from.0,
                            column: span.from.1,
                        }),
                    });
                }
                if let Some(document) = &mut self.document {
                    document.spans.set_page(page.raw);
                }
                self.recolor()
            }
            Err(code) => {
                self.failed(code);
                Vec::new()
            }
        }
    }

    /// The file's picture arrived; one for an older request than the latest - of the file before
    /// a reload - is dropped (RC-080-53).
    pub fn picture_read(&mut self, generation: u64, picture: Result<Picture, &'static str>) {
        if generation == self.picture_generation {
            self.pictured(picture);
        }
    }

    pub fn pictured(&mut self, picture: Result<Picture, &'static str>) {
        self.picture = match picture {
            Ok(picture) => Shown::Ready(picture),
            Err(code) => Shown::Failed(code),
        };
    }

    /// The window measured the rows that fit.
    pub fn set_rows(&mut self, rows: usize) -> Vec<Request> {
        let rows = rows.max(1);
        if rows == self.rows {
            return Vec::new();
        }
        self.rows = rows;
        self.show(Seek::Stay, None)
    }

    /// The scrollbar moved to `share` of the file.
    pub fn scrolled(&mut self, share: f32) -> Vec<Request> {
        self.show(Seek::Fraction(share), None)
    }

    /// Where the view is, as a share of the file, for the scrollbar.
    pub fn share(&self) -> f32 {
        match self.size() {
            0 => 0.0,
            size => (self.top as f64 / size as f64) as f32,
        }
    }

    /// The text editor's action: selecting and copying happen there, scrolling here, and an
    /// edit never.
    pub fn editor(&mut self, action: Action) -> Vec<Request> {
        match action {
            Action::Scroll { lines } if lines > 0 => {
                self.show(Seek::Down(lines.unsigned_abs() as usize), None)
            }
            Action::Scroll { lines } if lines < 0 => {
                self.show(Seek::Up(lines.unsigned_abs() as usize), None)
            }
            Action::Scroll { .. } | Action::Edit(_) => Vec::new(),
            other => {
                self.content.perform(other);
                Vec::new()
            }
        }
    }

    /// A key of the viewer; [`ListerKey::Close`], N and P are the window's.
    pub fn key(&mut self, key: ListerKey) -> Vec<Request> {
        self.note = None;
        if let Some(requests) = self.document_key(key) {
            return requests;
        }
        if let Some(requests) = self.preview_key(key) {
            return requests;
        }
        match key {
            ListerKey::Mode(mode) => self.set_mode(mode),
            ListerKey::Encoding(encoding) => self.set_encoding(encoding),
            ListerKey::NextEncoding => {
                let all = TextEncoding::ALL;
                let at = all
                    .iter()
                    .position(|e| *e == self.detected.encoding)
                    .unwrap_or(0);
                self.set_encoding(all[(at + 1) % all.len()])
            }
            ListerKey::Wrap => {
                self.wrap = !self.wrap;
                self.left = 0;
                self.show(Seek::Stay, None)
            }
            ListerKey::TabWidth => {
                let at = TAB_WIDTHS
                    .iter()
                    .position(|w| *w == self.tab_width)
                    .unwrap_or(0);
                self.tab_width = TAB_WIDTHS[(at + 1) % TAB_WIDTHS.len()];
                self.show(Seek::Stay, None)
            }
            ListerKey::Scroll(scroll) => self.scroll(scroll),
            ListerKey::Reload => vec![Request::Reload],
            ListerKey::Search => self.open_bar(super::find::BarKind::Search),
            ListerKey::GoTo => self.open_bar(super::find::BarKind::GoTo),
            ListerKey::FindNext => self.find_again(Direction::Forward),
            ListerKey::FindPrevious => self.find_again(Direction::Backward),
            ListerKey::ZoomIn | ListerKey::ZoomOut | ListerKey::Fit => {
                self.zoom_key(key);
                Vec::new()
            }
            ListerKey::RotateRight => {
                self.zoom.turns = (self.zoom.turns + 1) % 4;
                Vec::new()
            }
            ListerKey::RotateLeft => {
                self.zoom.turns = (self.zoom.turns + 3) % 4;
                Vec::new()
            }
            // The text size keys are the window's (`text_size.rs`); with the setting off they
            // do nothing.
            ListerKey::Close
            | ListerKey::NextFile
            | ListerKey::PreviousFile
            | ListerKey::NextView
            | ListerKey::Preview
            | ListerKey::SelectAll
            | ListerKey::Copy
            | ListerKey::TextLarger
            | ListerKey::TextSmaller
            | ListerKey::TextReset => Vec::new(),
        }
    }

    /// 1 to 4. The image mode only for a file that is one, or a document with a picture; the
    /// place is kept as a byte.
    pub(super) fn set_mode(&mut self, mode: ViewMode) -> Vec<Request> {
        let is_image =
            self.file.as_ref().is_some_and(|file| file.image.is_some()) || self.document_picture();
        if mode == self.mode || (mode == ViewMode::Image && !is_image) {
            return Vec::new();
        }
        self.mode = mode;
        self.left = 0;
        self.show(Seek::At(self.top), None)
    }

    /// A, S, 6, 8 or E: the text in another encoding; the image and hex views turn to text.
    fn set_encoding(&mut self, encoding: TextEncoding) -> Vec<Request> {
        let head = self.file.as_ref().map_or(&[][..], |file| &file.head[..]);
        self.detected = Detected::chosen(encoding, head);
        if self.mode != ViewMode::Binary {
            self.mode = ViewMode::Text;
        }
        self.show(Seek::At(self.top), None)
    }

    fn scroll(&mut self, scroll: Scroll) -> Vec<Request> {
        let page = self.rows.saturating_sub(1).max(1);
        let seek = match scroll {
            Scroll::LineUp => Seek::Up(1),
            Scroll::LineDown => Seek::Down(1),
            Scroll::PageUp => Seek::Up(page),
            Scroll::PageDown => Seek::Down(page),
            Scroll::Start => Seek::At(0),
            Scroll::End => Seek::End,
            Scroll::Left | Scroll::Right if self.wrap || self.mode != ViewMode::Text => {
                return Vec::new();
            }
            Scroll::Left => {
                self.left = self.left.saturating_sub(SIDE_STEP);
                Seek::Stay
            }
            Scroll::Right => {
                self.left += SIDE_STEP;
                Seek::Stay
            }
        };
        self.show(seek, None)
    }

    fn zoom_key(&mut self, key: ListerKey) {
        let zoom = &mut self.zoom;
        match key {
            ListerKey::Fit => {
                zoom.fit = !zoom.fit;
                zoom.scale = 1.0;
            }
            ListerKey::ZoomIn => {
                let scale = if zoom.fit {
                    1.0
                } else {
                    zoom.scale * ZOOM_STEP
                };
                zoom.scale = scale.min(ZOOM_MAX);
                zoom.fit = false;
            }
            _ => {
                let scale = if zoom.fit {
                    1.0
                } else {
                    zoom.scale / ZOOM_STEP
                };
                zoom.scale = scale.max(ZOOM_MIN);
                zoom.fit = false;
            }
        }
    }
}

#[cfg(test)]
#[path = "lister_tests.rs"]
mod tests;
