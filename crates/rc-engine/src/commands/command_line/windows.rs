//! The command line's shell on Windows: `cmd.exe` from the system folder, with its arguments
//! passed as written (`raw_arg`), since cmd does not parse them as other programs do.

use std::{os::windows::process::CommandExt, path::PathBuf, process::Command};

use super::cmd_arguments;

/// `cmd.exe` in the system folder, not one found on the search path.
fn cmd_exe() -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join("cmd.exe")
}

/// cmd running `line`; with `keep_open` its window stays after the line ends.
pub(super) fn shell_command(line: &str, keep_open: bool) -> Command {
    // console-window: allow (the command line shows its output in a console, as Total Commander)
    let mut command = Command::new(cmd_exe());
    command.raw_arg(cmd_arguments(line, keep_open));
    command
}
