//! The dialogs and windows that take a message before the window does, the top layer first.

mod access_keys;

use super::{App, Effect, Message, dialog_windows::DialogWindow};

impl App {
    /// The effects of the first dialog that takes `message`, or `None` when none does. The
    /// question of an MCP call goes over every other dialog, a host-key question over the rest:
    /// a call or a connection waits for a person.
    pub(super) fn intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        // The underline of the access keys follows Alt and its setting (RC-080-16).
        if let Some(effects) = self.access_keys_intercept(message) {
            return Some(effects);
        }
        // The self-update's own messages and events; every key and click for its quiet
        // (RC-100-05, RC-100-07).
        if let Some(effects) = self.update_intercept(message) {
            return Some(effects);
        }
        // The focus between the windows, and the keys of the queue's own window while it has the
        // keyboard: that window is modeless, beside every dialog of the main window (RC-010-14).
        if let Some(effects) = self.queue_window_first(message) {
            return Some(effects);
        }
        // A small question in a window of its own takes the input while it has the keys
        // (RC-080-23, `dialog_windows/questions.rs`); the engine's events go on below.
        if let Some(effects) = self.question_window_intercept(message) {
            return Some(effects);
        }
        if let Some(effects) = self.mcp_confirm_intercept(message) {
            return Some(effects);
        }
        if let Some(effects) = self.host_key_intercept(message) {
            return Some(effects);
        }
        if let Some(effects) = self.cert_trust_intercept(message) {
            return Some(effects);
        }
        if let Some(effects) = self.vault_offer_intercept(message) {
            return Some(effects);
        }
        // F10's quit and its question (RC-080-17).
        if let Some(effects) = self.quit_intercept(message) {
            return Some(effects);
        }
        // A large dialog in a modeless window of its own takes the input while that window has
        // the keyboard, and only then (RC-080-15, `dialog_windows/own.rs`); below, the main
        // window's chain leaves such a dialog's input alone (`reaches`).
        if let Some(effects) = self.own_window_intercept(message) {
            return Some(effects);
        }
        // The unlock prompt a connection asked for alone is a small question over every dialog
        // of the main window and over a modal dialog's window (RC-080-23).
        let prompt = self
            .connections
            .as_ref()
            .is_some_and(|manager| manager.prompt_only);
        if prompt && let Some(effects) = self.connections_intercept(message) {
            return Some(effects);
        }
        // The file operations in windows of their own - a job's question on top - before the
        // other dialogs, while none of those has the keyboard (RC-080-23).
        if self.file_ops_first(message)
            && let Some(effects) = self.file_ops_intercept(message)
        {
            return Some(effects);
        }
        let manager = self.reaches(DialogWindow::Connections, message);
        // "Run command" in its modal window of its own has the keys while it is open.
        let exec = manager || self.windowed(DialogWindow::SshExec);
        if exec && let Some(effects) = self.ssh_exec_intercept(message) {
            return Some(effects);
        }
        // A network drive dialog or a drive's context menu (RC-010-16).
        if let Some(effects) = self.drive_bar_intercept(message) {
            return Some(effects);
        }
        if let Some(effects) = self.palette_intercept(message) {
            return Some(effects);
        }
        if let Some(effects) = self.settings_intercept(message) {
            return Some(effects);
        }
        // The login-script editor in its own window takes its input above (RC-080-15).
        let editor = manager && self.reaches(DialogWindow::LoginScripts, message);
        if editor && let Some(effects) = self.login_scripts_intercept(message) {
            return Some(effects);
        }
        if manager && let Some(effects) = self.connections_intercept(message) {
            return Some(effects);
        }
        if let Some(effects) = self.viewer_intercept(message) {
            return Some(effects);
        }
        if self.reaches(DialogWindow::Search, message)
            && let Some(effects) = self.search_intercept(message)
        {
            return Some(effects);
        }
        if let Some(effects) = self.multi_rename_intercept(message) {
            return Some(effects);
        }
        // The diff window can open over a question of the file operations ("Compare").
        if self.reaches(DialogWindow::Compare, message)
            && let Some(effects) = self.compare_intercept(message)
        {
            return Some(effects);
        }
        // The main window's own layers take no input while a modal dialog's window locks it;
        // the drag question waits for the dialog, below the dialog's keys (RC-080-23); the file
        // operations, while they lie there as cards, too.
        if !self.main_reaches(message) {
            return None;
        }
        if let Some(effects) = self.file_ops_intercept(message) {
            return Some(effects);
        }
        if let Some(effects) = self.drag_intercept(message) {
            return Some(effects);
        }
        if let Some(effects) = self.queue_window_intercept(message) {
            return Some(effects);
        }
        None
    }
}
