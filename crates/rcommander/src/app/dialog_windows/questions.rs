//! The small questions in windows of their own (RC-080-23, owner 2026-10-19: the small questions
//! too, as in Total Commander): the MCP call's confirmation, a host key, a certificate, the offer
//! to remove the unused vault, quitting while jobs run, the unlock prompt a connection asked for
//! alone, an archive's password, the question before a large fetch and the confirmation before a
//! synchronization. While `appearance.dialog_windows` is on each is a window of the system with
//! its title, its place (`window.dialog.<name>`) and the app's icon; off, they stay the cards they
//! were (`dialog_windows.rs`, `question_layers`).
//!
//! One question has a window at a time - the top one in [`Question::ALL`], the order in which
//! their cards take the keys - and the others wait, as their cards waited under it: the window
//! is never behind another question's, a key reaches the question on screen, and Windows keeps
//! one owner chain. A newer question above it (an MCP call while a host key is asked) takes the
//! window's place; the one below opens again once it is answered.
//!
//! A question's window is owned by the window it would hide behind and locks it while it is open
//! (`question_window.rs`): the synchronization's confirmation by the compare window, every other
//! by the front modal dialog's window while one is open, else by the main window, which is
//! restored first - an owned window shows only with its owner. It comes to the front with the
//! keyboard and its taskbar button flashes until the person looks. Its input is the question's
//! ([`App::question_window_intercept`]) while it, the main window, the window it locks or none of
//! the app's windows has the keyboard; a modeless dialog's window (a Lister, find files, the
//! queue) works on beside it as beside a modal dialog. Escape, F10 and the title bar's X answer
//! as Escape does on the card: Deny, Cancel, Keep, skip. Each question keeps its safety: the MCP
//! call's window opens on Deny and only a click allows (RC-010-13, RC-080-12); a host key and a
//! certificate are answered only by the person (`GuiOnly`); the passwords stay secret types the
//! window's books never see.

use iced::{Element, Size};

use super::{DialogWindow, own::is_input};
use crate::{
    app::{
        App, Effect, Message, archive_ops, archive_ops::ArchiveDialog, connections, mcp_confirm,
    },
    i18n::t,
    theme::{Tokens, layout},
};

/// A small question with a window of its own while the dialogs have theirs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    /// An MCP call waits for Allow or Deny (RC-010-13).
    Mcp,
    /// A server's key never seen or changed (RC-050-07).
    HostKey,
    /// An FTPS certificate the system does not trust (RC-070-02).
    Certificate,
    /// The last secret went: remove the master password too (RC-050-03)?
    VaultOffer,
    /// Quitting while jobs run or wait (RC-080-17).
    Quit,
    /// The unlock prompt a connection asked for alone (RC-050-03).
    Unlock,
    /// An encrypted archive's password while it unpacks (RC-030-03).
    ArchivePassword,
    /// The question before a large fetch for a drag or the clipboard (RC-080-01).
    LargeFetch,
    /// The confirmation before a synchronization runs (RC-020-04).
    SyncConfirm,
}

impl Question {
    /// Every question, the top one first - the order their cards take the keys in
    /// (`intercepts.rs`): a call or a connection waits for a person, the person's own questions
    /// come after.
    pub const ALL: [Self; 9] = [
        Self::Mcp,
        Self::HostKey,
        Self::Certificate,
        Self::VaultOffer,
        Self::Quit,
        Self::Unlock,
        Self::ArchivePassword,
        Self::LargeFetch,
        Self::SyncConfirm,
    ];

    /// Whether the main window takes no input while it is open: every question but the
    /// synchronization's confirmation, which is the compare window's own - modeless beside the
    /// main window, it locks only that window.
    pub fn modal(self) -> bool {
        self != Self::SyncConfirm
    }

    /// The setting its position and size are kept under.
    pub fn key(self) -> &'static str {
        match self {
            Self::Mcp => "window.dialog.mcp_confirm",
            Self::HostKey => "window.dialog.host_key",
            Self::Certificate => "window.dialog.certificate",
            Self::VaultOffer => "window.dialog.vault_offer",
            Self::Quit => "window.dialog.quit",
            Self::Unlock => "window.dialog.vault_unlock",
            Self::ArchivePassword => "window.dialog.archive_password",
            Self::LargeFetch => "window.dialog.large_fetch",
            Self::SyncConfirm => "window.dialog.sync_confirm",
        }
    }

    /// Its size at its first opening: its card's width - small or medium - and its usual height.
    pub fn default_size(self) -> Size {
        match self {
            Self::Mcp => Size::new(layout::MEDIUM_DIALOG_WIDTH, 560.0),
            Self::HostKey => Size::new(layout::MEDIUM_DIALOG_WIDTH, 460.0),
            Self::Certificate => Size::new(layout::MEDIUM_DIALOG_WIDTH, 520.0),
            Self::VaultOffer => Size::new(layout::SMALL_DIALOG_WIDTH, 200.0),
            Self::Quit => Size::new(layout::SMALL_DIALOG_WIDTH, 180.0),
            Self::Unlock => Size::new(layout::SMALL_DIALOG_WIDTH, 300.0),
            Self::ArchivePassword => Size::new(layout::SMALL_DIALOG_WIDTH, 280.0),
            Self::LargeFetch => Size::new(layout::SMALL_DIALOG_WIDTH, 220.0),
            Self::SyncConfirm => Size::new(layout::SMALL_DIALOG_WIDTH, 320.0),
        }
    }

    /// The smallest it can be made: its text and its buttons still fit.
    pub fn min_size(self) -> Size {
        match self {
            Self::Mcp => Size::new(520.0, 360.0),
            Self::HostKey => Size::new(520.0, 340.0),
            Self::Certificate => Size::new(520.0, 380.0),
            Self::VaultOffer => Size::new(400.0, 170.0),
            Self::Quit => Size::new(400.0, 150.0),
            Self::Unlock => Size::new(420.0, 240.0),
            Self::ArchivePassword => Size::new(420.0, 220.0),
            Self::LargeFetch => Size::new(400.0, 180.0),
            Self::SyncConfirm => Size::new(420.0, 240.0),
        }
    }

    /// The control that takes the keyboard as its window opens: Deny for the MCP call, so a key
    /// typed as it appears denies (RC-080-16); the password field of the unlock prompt and of an
    /// archive. The other questions keep their own default - the safe button of a host key or a
    /// certificate, Cancel of quitting - which Enter takes without a focused control.
    pub fn first_field(self) -> Option<&'static str> {
        match self {
            Self::Mcp => Some(mcp_confirm::DENY_ID),
            Self::Unlock => Some(connections::VAULT_FIELD),
            Self::ArchivePassword => Some(archive_ops::PASSWORD_FIELD),
            Self::HostKey
            | Self::Certificate
            | Self::VaultOffer
            | Self::Quit
            | Self::LargeFetch
            | Self::SyncConfirm => None,
        }
    }
}

impl App {
    /// The question with a window of its own now: the top one asked while
    /// `appearance.dialog_windows` is on; `None` while none is asked, or while they are cards.
    pub(in crate::app) fn small_question(&self) -> Option<Question> {
        if !self.dialog_windows.own {
            return None;
        }
        Question::ALL
            .into_iter()
            .find(|question| self.asks(*question))
    }

    /// Whether `question` is asked now.
    fn asks(&self, question: Question) -> bool {
        match question {
            Question::Mcp => !self.mcp_questions.is_empty(),
            Question::HostKey => self.host_keys.shown().is_some(),
            Question::Certificate => self.cert_trust.shown().is_some(),
            Question::VaultOffer => self.vault_offer,
            Question::Quit => self.quit.asking.is_some(),
            Question::Unlock => self
                .connections
                .as_ref()
                .is_some_and(|manager| manager.prompt_only && manager.prompt.is_some()),
            Question::ArchivePassword => matches!(self.archive, Some(ArchiveDialog::Password(_))),
            Question::LargeFetch => self.drag.asking(),
            Question::SyncConfirm => self
                .compare
                .sync
                .as_ref()
                .is_some_and(|dialog| dialog.confirm.is_some()),
        }
    }

    /// The question in a window of its own that takes the input now: while its window, the main
    /// window, the window it locks or none of the app's windows has the keyboard - a modeless
    /// dialog's window works on beside it. The synchronization's confirmation takes only its own
    /// window's input and the compare window's, the main window's keys stay the panels'.
    pub(in crate::app) fn question_with_keys(&self) -> Option<Question> {
        let question = self.small_question()?;
        let focused = self.dialog_windows.focused_kind();
        let takes = match (question, focused) {
            (_, Some(DialogWindow::Question(_))) => true,
            (Question::SyncConfirm, focused) => focused == Some(DialogWindow::Compare),
            (_, None) => true,
            (_, Some(kind)) => kind.modal(),
        };
        takes.then_some(question)
    }

    /// The input of the question in a window of its own while it has the keys
    /// ([`App::question_with_keys`]): that question's, as its card took it; no key it leaves
    /// reaches anything under it. `None` for any other message.
    pub(in crate::app) fn question_window_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        if !is_input(message) {
            return None;
        }
        let taken = match self.question_with_keys()? {
            Question::Mcp => self.mcp_confirm_intercept(message),
            Question::HostKey => self.host_key_intercept(message),
            Question::Certificate => self.cert_trust_intercept(message),
            Question::VaultOffer => self.vault_offer_intercept(message),
            Question::Quit => self.quit_intercept(message),
            Question::Unlock => self.connections_intercept(message),
            Question::ArchivePassword => match message {
                Message::Key(chord) => Some(self.archive_key(chord)),
                _ => None,
            },
            Question::LargeFetch => self.drag_intercept(message),
            Question::SyncConfirm => self.compare_intercept(message),
        };
        Some(taken.unwrap_or_default())
    }

    /// What the window of `question` shows: the question on the card's surface - a host key or a
    /// certificate that changed with the danger border its card has; nothing once it is answered.
    pub(in crate::app) fn question_layers_of(
        &self,
        question: Question,
        tokens: &'static Tokens,
    ) -> Vec<Element<'_, Message>> {
        let (body, warning) = match question {
            Question::Mcp => (mcp_confirm::body(&self.mcp_questions, tokens), false),
            Question::HostKey => match self.host_key_body(tokens) {
                Some((body, warning)) => (Some(body), warning),
                None => (None, false),
            },
            Question::Certificate => match self.cert_trust_body(tokens) {
                Some((body, warning)) => (Some(body), warning),
                None => (None, false),
            },
            Question::VaultOffer => (self.vault_offer_body(), false),
            Question::Quit => (self.quit_body(), false),
            Question::Unlock => (
                self.connections
                    .as_ref()
                    .and_then(|manager| connections::prompt_body(manager, tokens)),
                false,
            ),
            Question::ArchivePassword => (
                self.archive
                    .as_ref()
                    .filter(|dialog| !archive_ops::windowed(dialog))
                    .map(|dialog| archive_ops::body(dialog, tokens)),
                false,
            ),
            Question::LargeFetch => (self.drag_question_body(tokens), false),
            Question::SyncConfirm => (self.sync_confirm_body(), false),
        };
        body.map(|body| layout::warning_window_body(body, warning, tokens))
            .into_iter()
            .collect()
    }

    /// The title of the window of `question`, as its card's head had it.
    pub(in crate::app) fn question_title(&self, question: Question) -> String {
        let title = match question {
            Question::Mcp => return mcp_confirm::title(&self.mcp_questions).unwrap_or_default(),
            Question::HostKey => self.host_key_title().unwrap_or_default(),
            Question::Certificate => self.cert_trust_title().unwrap_or_default(),
            Question::VaultOffer => t!("vault_offer.title"),
            Question::Quit => t!("quit.title"),
            Question::Unlock => self
                .connections
                .as_ref()
                .and_then(connections::prompt_title)
                .unwrap_or_default(),
            Question::ArchivePassword => t!("archive_ops.password_title"),
            Question::LargeFetch => t!("drag.fetch_title"),
            Question::SyncConfirm => t!("sync.confirm_title"),
        };
        title.to_owned()
    }
}

#[cfg(test)]
#[path = "questions_tests.rs"]
mod tests;
