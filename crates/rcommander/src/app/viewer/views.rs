//! A document's views switched and fed (RC-100-10): `7` or the list of views in the title row
//! steps between them as between Lister's own modes - a picture beside the text included
//! (RC-100-29, `output.rs`) - the answers of the source come in, and the setting
//! `plugins.viewer_token_colors` turns the colours on or off in every viewer.
//!
//! The coloured text is the Lister's text view with spans over it: only in the text mode, never
//! in binary or hex, and only for the lines on screen. In the tree the arrows, Page Up and Down,
//! Home and End are the tree's; a mode, an encoding or a search goes back to the text first.

use iced::Task;
use rc_db::Db;
use rc_engine::{
    settings::{VIEWER_TOKEN_COLORS_KEY, stored_bool},
    viewer::ViewMode,
};

use super::{
    ViewerMessage,
    document::{DocView, DocumentMessage},
    keys::{ListerKey, Scroll},
    lister::{Lister, Request},
    load::Seek,
    markdown::Markdown,
    quick::{Quick, QuickShown},
    tree::Step,
    wrap,
};
use crate::{
    app::{App, Effect, Message},
    i18n,
    theme::ROW_HEIGHT,
};

/// `plugins.viewer_token_colors`: whether a document's coloured text shows its colours. On until
/// the setting is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenColors(pub bool);

impl Default for TokenColors {
    fn default() -> Self {
        Self(true)
    }
}

/// Reads `plugins.viewer_token_colors`; on without it.
pub async fn load(db: Db) -> bool {
    stored_bool(&db, VIEWER_TOKEN_COLORS_KEY).await
}

impl Lister {
    /// Whether the body is the document's tree.
    pub fn tree_shown(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|document| document.view() == DocView::Tree)
    }

    /// A key while a document is shown: `7` steps to its next view, the tree takes its keys.
    /// `None` for a key the Lister handles as for any file.
    pub(super) fn document_key(&mut self, key: ListerKey) -> Option<Vec<Request>> {
        let document = self.document.as_mut()?;
        if key == ListerKey::NextView {
            let view = document.next_view();
            return Some(self.apply_view(view));
        }
        if document.view() != DocView::Tree {
            return None;
        }
        let step = match key {
            ListerKey::Scroll(scroll) => match scroll {
                Scroll::LineUp => Step::Up,
                Scroll::LineDown => Step::Down,
                Scroll::PageUp => Step::PageUp,
                Scroll::PageDown => Step::PageDown,
                Scroll::Start => Step::Home,
                Scroll::End => Step::End,
                Scroll::Left => Step::Close,
                Scroll::Right => Step::Open,
            },
            ListerKey::Mode(_)
            | ListerKey::Preview
            | ListerKey::Encoding(_)
            | ListerKey::NextEncoding
            | ListerKey::Search
            | ListerKey::GoTo
            | ListerKey::FindNext
            | ListerKey::FindPrevious => {
                // Back to the text, where the key does what it does for any file.
                document.leave_tree();
                let mut requests = self.key(key);
                requests.extend(self.recolor());
                return Some(requests);
            }
            _ => return None,
        };
        Some(children_of(document.tree.step(step)))
    }

    /// The view `view` shown, and what it needs read or asked.
    pub(super) fn apply_view(&mut self, view: DocView) -> Vec<Request> {
        let mut requests = Vec::new();
        match view {
            // The text is written first (RC-100-29); `output.rs` shows it when it is there.
            view if view.is_text() && self.text_waits() => requests.extend(self.ask_text()),
            DocView::Tree => {
                self.hide_preview();
                if let Some(asked) = self.document.as_mut().and_then(|d| d.tree.start()) {
                    requests.push(Request::Children(asked));
                }
            }
            DocView::Text | DocView::Highlighted => {
                self.hide_preview();
                if self.mode == ViewMode::Image {
                    self.mode = ViewMode::Text;
                    self.left = 0;
                    requests.extend(self.show(Seek::At(self.top), None));
                }
            }
            DocView::Markdown if self.preview().is_none() && !self.preview_waiting() => {
                requests.extend(self.toggle_preview());
            }
            DocView::Markdown => {}
            DocView::Image => {
                self.hide_preview();
                requests.extend(self.set_mode(ViewMode::Image));
            }
        }
        requests.extend(self.recolor());
        requests
    }

    /// The page's colours again: the tokens of lines without any asked for, the spans laid on the
    /// text as it shows now. No colour, and nothing asked, outside the coloured text view, in the
    /// binary and hex modes and while the setting is off.
    pub(super) fn recolor(&mut self) -> Vec<Request> {
        let text = self.mode == ViewMode::Text && self.preview().is_none();
        let (tab_width, left) = (self.tab_width, if self.wrap { 0 } else { self.left });
        let Some(document) = &mut self.document else {
            return Vec::new();
        };
        let on = text && document.colors && document.view() == DocView::Highlighted;
        let wanted = if on {
            document.spans.wanted()
        } else {
            Vec::new()
        };
        document.spans.refresh(on, tab_width, left);
        if wanted.is_empty() {
            Vec::new()
        } else {
            vec![Request::Tokens(wanted)]
        }
    }

    /// An answer or an input of the document's views; an answer for another document is dropped.
    pub(super) fn document_message(&mut self, message: DocumentMessage) -> Vec<Request> {
        let Some(document) = &mut self.document else {
            return Vec::new();
        };
        match message {
            DocumentMessage::Tokens(serial, starts, answer) => {
                if serial != document.serial() {
                    return Vec::new();
                }
                document.spans.arrived(&starts, answer);
                self.recolor()
            }
            DocumentMessage::Children(serial, parent, answer) => {
                if serial == document.serial()
                    && let Err(code) = document.tree.arrived(parent, answer)
                {
                    self.note = Some(i18n::error_text(code));
                }
                Vec::new()
            }
            DocumentMessage::Text(serial, answer) => self.text_arrived(serial, answer),
            DocumentMessage::Picture(serial, answer) => self.picture_arrived(serial, answer),
            DocumentMessage::View(view) => {
                if document.choose(view) {
                    self.apply_view(view)
                } else {
                    Vec::new()
                }
            }
            DocumentMessage::TreeClick(row) => children_of(document.tree.click(row)),
            DocumentMessage::TreeScroll(rows) => {
                document.tree.scroll(rows);
                Vec::new()
            }
            DocumentMessage::TreeViewport(height) => {
                // A count of rows: the cast cannot leave the window.
                document
                    .tree
                    .set_height((height / ROW_HEIGHT).floor().max(1.0) as usize);
                Vec::new()
            }
        }
    }

    /// A Markdown preview hidden behind the document's other views.
    fn hide_preview(&mut self) {
        if let Some(Markdown::Asked { show } | Markdown::Ready { shown: show, .. }) =
            &mut self.markdown
        {
            *show = false;
        }
    }
}

/// The question for the children of the node a key or a click unfolded, if any.
fn children_of(node: Option<u64>) -> Vec<Request> {
    node.map(|id| Request::Children(Some(id)))
        .into_iter()
        .collect()
}

impl App {
    /// Reads `plugins.viewer_token_colors` (the start, and after it changed).
    pub(in crate::app) fn token_colors_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(load(db), |on| wrap(ViewerMessage::TokenColors(on)))
    }

    /// The setting read: every open document shows or drops its colours.
    pub(super) fn on_token_colors(&mut self, on: bool) -> Vec<Effect> {
        self.viewers.token_colors = TokenColors(on);
        let mut ids: Vec<u64> = self.viewers.open.iter().map(|lister| lister.id).collect();
        if let Some(Quick {
            shown: QuickShown::File(lister),
            ..
        }) = &self.viewers.quick
        {
            ids.push(lister.id);
        }
        let mut effects = Vec::new();
        for id in ids {
            let Some(lister) = self.viewers.lister_mut(id) else {
                continue;
            };
            let Some(document) = &mut lister.document else {
                continue;
            };
            document.colors = on;
            let requests = lister.recolor();
            effects.extend(self.run(id, requests));
        }
        effects
    }
}

#[cfg(test)]
#[path = "views_tests.rs"]
mod tests;
