//! A document's picture and text in one Lister (RC-100-29): the first output is written before
//! the Lister shows the document, the other one only when the person first switches to it with
//! `7` or the list of views - Quick View, moving on fast, pays for one.
//!
//! The picture never is the Lister's file: the image view asks the document's source for it,
//! decoded off the UI thread, and keeps it while the person switches back and forth. The text is
//! the Lister's file once it is written; until then the Lister holds the file itself, and the
//! first switch to a text view asks for the text and keeps the view shown until it is there.
//! A second output that cannot be written is a notice in the status line; its views go, and the
//! first view goes on working.

use rc_engine::viewer::ViewMode;

use super::{
    document::{DocView, TextFile},
    lister::{Lister, Request, Shown},
    load::{Opened, Picture, Seek},
};
use crate::i18n::{self, t};

impl Lister {
    /// Whether the image view shows the document's picture rather than the file's.
    pub(super) fn document_picture(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|document| document.offers(DocView::Image))
    }

    /// Whether the document's text is not the Lister's file yet.
    pub(super) fn text_waits(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|document| document.text_file() != TextFile::Shown)
    }

    /// The first switch to a text view: the text asked for once, the view shown kept until it
    /// is there.
    pub(super) fn ask_text(&mut self) -> Vec<Request> {
        let Some(document) = &mut self.document else {
            return Vec::new();
        };
        if !document.ask_text() {
            return Vec::new();
        }
        self.note = Some(t!("viewer.loading").to_owned());
        vec![Request::Text]
    }

    /// The document's text, written and opened: the Lister's file from now on, shown when a text
    /// view is chosen. A failure is the notice, and the text views go.
    pub(super) fn text_arrived(
        &mut self,
        serial: u64,
        answer: Result<Opened, &'static str>,
    ) -> Vec<Request> {
        let Some(document) = &mut self.document else {
            return Vec::new();
        };
        if serial != document.serial() {
            return Vec::new();
        }
        let opened = match answer {
            Ok(opened) => opened,
            Err(code) => {
                document.drop_views(DocView::is_text);
                let view = document.view();
                let requests = self.apply_view(view);
                self.note = Some(i18n::error_text(code));
                return requests;
            }
        };
        document.text_shown();
        document.spans.clear();
        let view = document.view();
        self.note = None;
        self.detected = opened.detected;
        self.file = Some(opened);
        self.top = 0;
        self.left = 0;
        let mut requests = Vec::new();
        if self.mode != ViewMode::Image {
            requests.extend(self.show(Seek::At(0), None));
        }
        requests.extend(self.apply_view(view));
        requests
    }

    /// The document's picture, decoded. A picture that is the document's first output fails as
    /// a file's does, in the image view; a second one is the notice, and the image view goes.
    pub(super) fn picture_arrived(
        &mut self,
        serial: u64,
        answer: Result<Picture, &'static str>,
    ) -> Vec<Request> {
        let Some(document) = &mut self.document else {
            return Vec::new();
        };
        if serial != document.serial() {
            return Vec::new();
        }
        match answer {
            Err(code) if document.views().first() != Some(&DocView::Image) => {
                self.picture = Shown::Failed(code);
                document.drop_views(|view| view == DocView::Image);
                let view = document.view();
                let requests = self.apply_view(view);
                self.note = Some(i18n::error_text(code));
                requests
            }
            answer => {
                self.pictured(answer);
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
