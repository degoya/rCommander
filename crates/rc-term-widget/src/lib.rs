//! rCommander's terminal widget: an iced widget that draws an `rc-term` session and sends the
//! keyboard and the mouse to it.
//!
//! Forked from [iced_term](https://github.com/Harzu/iced_term) 0.8.0 by Ilia Shvyrialkin (MIT,
//! `LICENSE-iced_term` beside this crate), as ADR 0001 decided. What the fork changes:
//!
//! - **No PTY of its own.** iced_term started a process through `alacritty_terminal`'s event
//!   loop. Here a [`Terminal`] renders the `alacritty_terminal::Term` of an
//!   [`rc_term::TermSession`] - fed by whatever back end the session has (a local PTY, WSL, an
//!   SSH channel later) - and writes keys, pastes and mouse reports through the session.
//! - **Drawn where it is.** iced_term drew at window coordinates into a frame the size of the
//!   viewport, so inside a clipping container that does not start at the window's origin (the
//!   right panel) it stayed blank. The fork draws at the widget's origin and translates.
//! - **No panic when a session ends.** The redraw subscription ([`Terminal::subscription`])
//!   ends with the session instead of panicking on a closed channel.
//! - **Reserved keys** ([`ReservedKey`]): the app's own chords (the terminal toggle) never reach
//!   the shell, matched as the app's keymap reads keys.
//! - **Paste asks** when the text holds a line break or another control character but a tab
//!   ([`Event::Paste`]), copy and paste use the session's selection, Shift+PageUp/PageDown
//!   scroll the history, Windows pastes on a right click; Ctrl+C and a right click copy a
//!   selection, and so can letting go of the mouse;
//!   Ctrl+V pastes text, Shift selects while the program takes the mouse, and a program's OSC 52
//!   copy reaches the app ([`Event::Copy`], [`CopySettings`]).
//! - **Drawn without the lock.** A frame copies the cells that show anything while the terminal
//!   is locked and draws them after (RC-080-46); a wide glyph spans its two cells, a combining
//!   mark stays with its glyph.
//! - Dropped: links (opening URLs from program output), the `open` and `anyhow` dependencies.

mod bindings;
mod draw;
mod font;
mod input;
mod snapshot;
mod terminal;
mod theme;
mod view;

pub use terminal::{CopySettings, Event, FontSettings, ReservedKey, Settings, Terminal};
pub use theme::ColorPalette;
pub use view::TerminalView;
