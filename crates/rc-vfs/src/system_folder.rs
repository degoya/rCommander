//! Where Windows keeps its own programs (`wsl.exe`, `cmd.exe`, Windows PowerShell): the
//! `System32` folder of `%SystemRoot%` (RC-080-65).
//!
//! The variable counts only as an absolute path; unset, empty or relative, the folder is under
//! [`DEFAULT_SYSTEM_ROOT`]. A program is always started by its full path from here, never by a
//! bare name that a search could find in the current folder or early on `PATH`. The rule is
//! portable, so it is tested on every platform; `rc-term` and `rc-engine` read it from here.

use std::path::{Path, PathBuf};

/// Windows' folder when `SystemRoot` names none that can be trusted.
pub const DEFAULT_SYSTEM_ROOT: &str = r"C:\Windows";

/// `<system root>\System32` for the `SystemRoot` value `system_root`.
#[must_use]
pub fn system32(system_root: Option<&Path>) -> PathBuf {
    system_root
        .filter(|root| root.is_absolute())
        .map_or_else(|| PathBuf::from(DEFAULT_SYSTEM_ROOT), Path::to_path_buf)
        .join("System32")
}

/// [`system32`] of this process's `SystemRoot`.
#[must_use]
pub fn system32_from_env() -> PathBuf {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from);
    system32(root.as_deref())
}

#[cfg(test)]
#[path = "system_folder_tests.rs"]
mod tests;
