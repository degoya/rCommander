//! Escape cancels every dialog, always (owner, 2026-10-18; RC-080-15) - the one place for it.
//!
//! A dialog's close X ([`Message::close_dialog`]) and the title bar's X of a dialog in a window
//! of its own send Escape; each dialog takes Escape as its Cancel or Close. A text field that has
//! the keyboard uses Escape itself (iced's `text_input` lets go of the keys and captures it), so
//! the key would reach the dialog as [`Message::KeyCaptured`], which many dialogs ignore: while a
//! dialog is open, a captured Escape counts as a plain one ([`App::escape_to_dialog`]). Without a
//! dialog it stays captured - a terminal's Escape is the shell's, the command line's clears it.
//!
//! What Escape does where a dialog has more than one layer is the dialog's own, and written in
//! `docs/design.md` ("Keyboard first"): a question inside a dialog (a delete confirmation, the
//! master-password prompt, a name typed into the tree, the synchronize confirmation) goes first;
//! a running search or comparison stops first; a progress window goes to the background; a job's
//! conflict or error question is cancelled; an MCP question is denied.

use rc_engine::Chord;

use super::{App, Message, dialog_windows::DialogWindow};

/// The Escape key without modifiers.
pub fn escape() -> Chord {
    Chord::plain("Escape").expect("Escape is a named key")
}

/// The Enter key without modifiers: a dialog's default button (RC-080-16).
pub fn enter() -> Chord {
    Chord::plain("Enter").expect("Enter is a named key")
}

impl App {
    /// `message`, with a captured Escape turned into a plain one while a dialog is open.
    pub(super) fn escape_to_dialog(&self, message: Message) -> Message {
        match message {
            Message::KeyCaptured(chord) if chord == escape() && self.dialog_open() => {
                Message::Key(chord)
            }
            message => message,
        }
    }

    /// Whether a dialog, a question, a menu or a popup is open - something Escape closes. A
    /// large dialog in a modeless window of its own counts while that window has the keyboard
    /// (`dialog_windows/own.rs`).
    pub(super) fn dialog_open(&self) -> bool {
        let manager = self.has_keys(DialogWindow::Connections);
        let panels = self.settings.is_some()
            || self.database.is_some()
            || self.properties.is_some()
            || self.quick.is_some()
            || self.archive.is_some()
            || self.checksum_has_keys()
            || (manager && self.connections.is_some())
            || self.ssh_exec_has_keys()
            || self.login_scripts_have_keys()
            || self.multi_rename.is_some()
            || self.palette.is_some()
            || self.queue_has_keys()
            || self.context_menu.is_some()
            || self.dialog_windows.menu_bar.open.is_some()
            || (self.search.as_ref().is_some_and(|dialog| dialog.open)
                && self.has_keys(DialogWindow::Search));
        let questions = self.vault_offer
            || self.quit.asking.is_some()
            || !self.mcp_questions.is_empty()
            || self.host_keys.shown().is_some()
            || self.cert_trust.shown().is_some()
            || self.file_ops.modal()
            || self.drag.asking()
            || self.terms.paste.is_some()
            || self.terms.picker.is_some()
            // A question in a window of its own, the synchronization's confirmation too.
            || self.question_with_keys().is_some();
        let compare = self.compare.diff.is_some() || self.compare.sync.is_some();
        let overlays = (compare && self.has_keys(DialogWindow::Compare))
            || self.lister_has_keys()
            || self.tabs_have_keys()
            || self.drive_bar.menu.is_some()
            || self.drive_bar.context.is_some()
            || self.drive_bar.dialog.is_some();
        panels || questions || overlays
    }
}

#[cfg(test)]
#[path = "escape_tests.rs"]
mod tests;
