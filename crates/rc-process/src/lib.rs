//! What every program rCommander starts has in common.
//!
//! Window: on Windows a console program started by a process without a console (the GUI binary)
//! opens a console window of its own. [`NoConsoleWindow`] is the one way every spawn in the
//! workspace avoids it; `crates/rcommander/tests/repo_lints/no_console_window.rs` fails on a
//! `Command::new` that does not use it.

/// `CREATE_NO_WINDOW`: a console program started with it gets a console of its own that is
/// never shown, and the console programs it starts in turn share that hidden console.
///
/// Public for a caller that has to combine it with other creation flags; every other spawn calls
/// [`NoConsoleWindow::no_console_window`], because `creation_flags` replaces the flags set before
/// rather than adding to them.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Starts a program without a console window on Windows; nothing changes elsewhere.
///
/// Spawns that must show a window on purpose (an external editor, a terminal the user asked
/// for) do not use this and carry an allow comment instead (see the repo lint).
pub trait NoConsoleWindow {
    /// Sets [`CREATE_NO_WINDOW`] as the creation flags on Windows; a no-op elsewhere.
    fn no_console_window(&mut self) -> &mut Self;
}

impl NoConsoleWindow for tokio::process::Command {
    fn no_console_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        self.creation_flags(CREATE_NO_WINDOW);
        self
    }
}

impl NoConsoleWindow for std::process::Command {
    fn no_console_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(self, CREATE_NO_WINDOW);
        self
    }
}

/// `CREATE_NEW_PROCESS_GROUP`: a Ctrl-C meant for the starter does not reach the program.
pub const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
/// `CREATE_BREAKAWAY_FROM_JOB`: the program leaves the starter's job, so it outlives it.
pub const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

/// Starts a program that outlives the one that started it (the updater of RC-100-06 and the
/// version it starts): on Windows [`CREATE_NO_WINDOW`] with a process group of its own and, with
/// `breakaway`, outside the starter's job; on Unix a process group of its own. Here because only
/// this file sets creation flags (`repo_lints/no_console_window.rs`).
pub trait Detached {
    /// Sets the flags; a job that allows no breakaway refuses the spawn with error 5, and the
    /// caller tries again with `breakaway` off.
    fn detached(&mut self, breakaway: bool) -> &mut Self;
}

impl Detached for std::process::Command {
    fn detached(&mut self, breakaway: bool) -> &mut Self {
        #[cfg(windows)]
        {
            let job = if breakaway {
                CREATE_BREAKAWAY_FROM_JOB
            } else {
                0
            };
            std::os::windows::process::CommandExt::creation_flags(
                self,
                CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | job,
            );
        }
        #[cfg(unix)]
        {
            let _ = breakaway;
            std::os::unix::process::CommandExt::process_group(self, 0);
        }
        self
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
