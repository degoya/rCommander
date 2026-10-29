//! The IO side of a terminal, whatever carries it: a local PTY, a WSL shell, an SSH channel.
//!
//! A back end is started by its own constructor ([`crate::spawn_local`] for a local shell) and
//! hands back a [`Spawned`]: the [`TermBackend`] that takes input, size and the end, and a channel
//! of [`BackendEvent`]s - the program's output in order, then exactly one
//! [`BackendEvent::Exited`], then the channel closes. Neither side blocks the caller: input and
//! size changes are queued, output arrives on the channel, so an async back end (a russh channel)
//! and a blocking one (a PTY read on a thread) look the same to [`crate::TermSession`].

use alacritty_terminal::grid::Dimensions;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::TermError;

mod ssh;

pub use ssh::{SshTermChannel, spawn_ssh};

/// A terminal's size in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TermSize {
    pub rows: u16,
    pub cols: u16,
}

impl TermSize {
    /// The most rows and the most columns a terminal has: well beyond any screen, and a grid
    /// (with its scrollback) a caller cannot blow up to 65535 by 65535 cells (RC-080-42).
    pub const MAX: u16 = 1000;

    /// A size of `rows` by `cols`.
    ///
    /// # Errors
    ///
    /// [`TermError::InvalidSize`] when either is zero or above [`TermSize::MAX`].
    pub fn new(rows: u16, cols: u16) -> Result<Self, TermError> {
        let allowed = 1..=Self::MAX;
        if !allowed.contains(&rows) || !allowed.contains(&cols) {
            return Err(TermError::InvalidSize);
        }
        Ok(Self { rows, cols })
    }
}

impl Default for TermSize {
    /// The classic 24 by 80.
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

/// The size as `alacritty_terminal` measures it: the screen only, the scrollback is the
/// terminal's own.
impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        usize::from(self.rows)
    }

    fn columns(&self) -> usize {
        usize::from(self.cols)
    }
}

/// What a back end reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    /// Bytes the program wrote, in order.
    Output(Vec<u8>),
    /// The program ended, with its exit code when one is known (a killed program or a channel
    /// that closed without a status has none). The last event.
    Exited(Option<u32>),
}

/// The input side of a running program. Methods never block: they queue and return.
pub trait TermBackend: Send + Sync + 'static {
    /// Queues bytes for the program: keystrokes, pasted text, the terminal's own replies.
    ///
    /// # Errors
    ///
    /// [`TermError::Exited`] once the program has ended.
    fn write(&self, bytes: Vec<u8>) -> Result<(), TermError>;

    /// Tells the program its new size (a PTY resize, an SSH `window-change`).
    ///
    /// # Errors
    ///
    /// [`TermError::Exited`] once the program has ended; [`TermError::Pty`] when the resize
    /// itself fails.
    fn resize(&self, size: TermSize) -> Result<(), TermError>;

    /// Ends the program (kills the process, closes the channel). [`BackendEvent::Exited`]
    /// follows on the event channel. Calling it again, or after the end, does nothing.
    fn shutdown(&self);

    /// The process id of a local program, for diagnostics and tests.
    fn process_id(&self) -> Option<u32> {
        None
    }
}

/// A started back end: its input side and its event channel.
pub struct Spawned {
    pub backend: Box<dyn TermBackend>,
    pub events: mpsc::Receiver<BackendEvent>,
}

impl std::fmt::Debug for Spawned {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Spawned")
            .field("process_id", &self.backend.process_id())
            .finish_non_exhaustive()
    }
}
