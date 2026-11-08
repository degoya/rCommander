//! Selecting and copying the whole text of the text view (RC-110-06). The read-only
//! `text_editor` holds only the page that fits (`lister.rs`), so its own Ctrl+A took that page
//! and Ctrl+C copied it. Ctrl+A now selects the whole text, as Total Commander's Lister does:
//! the selection stays while the view scrolls, each page shows all its text selected, and
//! Ctrl+C copies the file's text, read and decoded off the UI thread, up to [`COPY_LIMIT`]. A
//! click, a drag or a search hit selects in the page again. The binary and hex views select and
//! copy their page; the Markdown preview selects its own blocks (`select.rs`).

use std::sync::Arc;

use iced::{
    keyboard::Key,
    widget::text_editor::{Action, Binding, KeyPress, Status},
};
use rc_engine::viewer::{Detected, FileWindow, ViewMode, encoding};

use super::{
    ViewerMessage,
    keys::ListerKey,
    lister::{Lister, Request},
    load::Seek,
    markdown::PREVIEW_LIMIT,
    wrap,
};
use crate::app::{Effect, Message};

/// The largest file whose whole text Ctrl+C copies: what `files_read_text` and the Markdown
/// preview read at most.
pub const COPY_LIMIT: u64 = PREVIEW_LIMIT;
/// The error codes of copying, each with its `error.<code>` text.
#[cfg(test)]
pub const ALL_CODES: &[&str] = &["viewer_copy_too_large"];

impl Lister {
    /// The text editor's action: selecting happens there, scrolling here, and an edit never. A
    /// selection made in the page ends the whole one.
    pub fn editor(&mut self, action: Action) -> Vec<Request> {
        match action {
            Action::Scroll { lines } if lines > 0 => {
                self.show(Seek::Down(lines.unsigned_abs() as usize), None)
            }
            Action::Scroll { lines } if lines < 0 => {
                self.show(Seek::Up(lines.unsigned_abs() as usize), None)
            }
            Action::Scroll { .. } | Action::Edit(_) => Vec::new(),
            Action::SelectAll => self.select_all(),
            other => {
                self.whole = false;
                self.content.perform(other);
                Vec::new()
            }
        }
    }

    /// Ctrl+A: the whole text in the text view, the page in the binary and hex views; nothing
    /// while the preview or a tree shows.
    pub(super) fn select_all(&mut self) -> Vec<Request> {
        if self.editor_shown() {
            self.whole = self.mode == ViewMode::Text;
            self.content.perform(Action::SelectAll);
        }
        Vec::new()
    }

    /// Ctrl+C: the whole text read and copied, or what is selected in the page.
    pub(super) fn copy(&mut self) -> Vec<Request> {
        if !self.editor_shown() {
            return Vec::new();
        }
        if self.whole_selected() {
            return vec![Request::CopyAll];
        }
        self.content
            .selection()
            .map(Request::Copy)
            .into_iter()
            .collect()
    }

    /// Whether Ctrl+A took the whole text and nothing was selected in the page since.
    pub fn whole_selected(&self) -> bool {
        self.whole && self.mode == ViewMode::Text && self.file.is_some()
    }

    /// A new page shows all its text selected while the whole text is.
    pub(super) fn keep_whole(&mut self) {
        if self.whole_selected() {
            self.content.perform(Action::SelectAll);
        }
    }

    /// The whole text read for Ctrl+C: on the clipboard, or the reason in the status line.
    pub(super) fn copied(&mut self, copied: Result<String, &'static str>) -> Vec<Effect> {
        match copied {
            Ok(text) => vec![Effect::Task(iced::clipboard::write(text))],
            Err(code) => {
                self.failed(code);
                Vec::new()
            }
        }
    }

    /// Whether the text editor is what the window shows: not the preview, not a tree, not the
    /// picture.
    fn editor_shown(&self) -> bool {
        self.mode != ViewMode::Image
            && self.preview().is_none()
            && !self
                .document
                .as_ref()
                .is_some_and(|document| document.view().is_tree())
    }
}

/// The whole text of `file` in `detected`'s encoding, its byte-order mark left out; a file over
/// [`COPY_LIMIT`] is refused.
pub async fn whole_text(file: Arc<FileWindow>, detected: Detected) -> Result<String, &'static str> {
    if file.size() > COPY_LIMIT {
        return Err("viewer_copy_too_large");
    }
    let len = usize::try_from(file.size()).map_err(|_| "viewer_copy_too_large")?;
    let bytes = file
        .read_uncached(0, len)
        .await
        .map_err(|error| error.code())?;
    let start = usize::try_from(detected.bom_len)
        .unwrap_or(0)
        .min(bytes.len());
    Ok(encoding::decode(&bytes[start..], detected.encoding))
}

/// Lister's editor reads only: Ctrl+A and Ctrl+C while it has the keyboard are lister `id`'s
/// keys, which take the whole text; every other key is the viewer's.
pub fn lister_keys(id: u64, press: KeyPress) -> Option<Binding<Message>> {
    if !matches!(press.status, Status::Focused { .. }) || !press.modifiers.command() {
        return None;
    }
    let key = match press.key.as_ref() {
        Key::Character("a") => ListerKey::SelectAll,
        Key::Character("c") => ListerKey::Copy,
        _ => return None,
    };
    Some(Binding::Custom(wrap(ViewerMessage::Key(id, key))))
}

#[cfg(test)]
#[path = "copy_tests.rs"]
mod tests;
