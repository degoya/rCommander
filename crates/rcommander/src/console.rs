//! The console of the program that started rCommander, for the subcommands that print.
//!
//! On Windows rCommander is a GUI program (`windows_subsystem = "windows"`): it starts with no
//! console, so what `rcommander backup` prints would vanish. It attaches to its parent's console
//! instead, when the parent has one; output the parent redirected already has its handles and
//! keeps them. A shell does not wait for a GUI program, so the prompt may come back before the
//! output (`start /wait` in cmd, `Start-Process -Wait` in PowerShell wait for it and its exit
//! code). Elsewhere a program always has its parent's terminal and this does nothing.

#[cfg(windows)]
mod windows;

/// Attaches to the parent process's console, if there is one; nothing happens otherwise.
pub fn attach_parent() {
    #[cfg(windows)]
    windows::attach_parent();
}
