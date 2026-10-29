//! A session's input held while something else talks to the program - the login script of an
//! SSH terminal (RC-060-03) - and text shown in the terminal without the program.
//!
//! Only what is typed is held ([`TermSession::write`]: the widget's keys, MCP's input); the
//! terminal's own replies (cursor reports, device attributes) still reach the program, and so
//! does a [`crate::Tap`]'s input.

use std::sync::Arc;

use alacritty_terminal::vte::ansi::Processor;

use super::{InputHook, TermEvent, TermSession, lock};

impl TermSession {
    /// Holds typed input until [`Self::release_input`]: [`Self::write`] hands the bytes to
    /// `hook` (which may, say, cancel the script on Ctrl+C) and answers
    /// [`crate::TermError::Busy`]. A second hold replaces the first.
    pub fn hold_input(&self, hook: impl Fn(&[u8]) + Send + Sync + 'static) {
        let hook: InputHook = Arc::new(hook);
        *lock(&self.held) = Some(hook);
    }

    /// Typed input goes to the program again.
    pub fn release_input(&self) {
        *lock(&self.held) = None;
    }

    /// Whether typed input is held.
    #[cfg(test)]
    #[must_use]
    pub fn input_held(&self) -> bool {
        lock(&self.held).is_some()
    }

    /// Shows `text` in the terminal as if the program had written it, and sends nothing: a
    /// notice of rCommander's own, such as a login script that timed out. `\r\n` ends a line.
    pub fn show_local(&self, text: &str) {
        let mut parser: Processor = Processor::new();
        parser.advance(&mut *self.shared.term.lock(), text.as_bytes());
        self.shared.notify(TermEvent::Wakeup);
    }
}

#[cfg(test)]
#[path = "hold_tests.rs"]
mod tests;
