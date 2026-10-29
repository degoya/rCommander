//! How this installation came to be, and therefore whether it updates itself (RC-100-06).
//!
//! Only the portable package installs itself; there is no installer, package manager or
//! container. Detection, first match wins:
//!
//! 1. **The marker file** [`INSTALL_KIND_FILE`] (`install-kind`) beside the program - room for a
//!    later installer: UTF-8 text, the first line that is neither empty nor a `#` comment is the
//!    kind; anything but `portable` is [`InstallKind::Unknown`].
//! 2. **`VERSION.txt` beside the program** (or in a macOS bundle's `Contents/Resources/`), as
//!    `scripts/package-*.sh` writes it, in a folder this process may write: [`InstallKind::Portable`].
//! 3. Otherwise [`InstallKind::Unknown`] with the reason the window shows: a build from source,
//!    the copy of `scripts/run-windows.sh` (no `VERSION.txt`), or a folder like
//!    `C:\Program Files` that cannot be written. It is offered the download only.
//!
//! Copied and reduced from rDownloader's `crates/rd-update/src/install_kind.rs` at commit
//! 26c8592a6: `portable` and `unknown`.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The marker a later installer writes beside the program.
pub const INSTALL_KIND_FILE: &str = "install-kind";
/// What every package carries beside the program.
const VERSION_FILE: &str = "VERSION.txt";
/// The most of a marker file that is read.
const MAX_MARKER_BYTES: u64 = 1024;

/// How this installation was installed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallKind {
    /// The zip or `.tar.gz` package, unpacked by hand: updates itself.
    Portable,
    /// Anything else: offered the download only.
    Unknown,
}

impl InstallKind {
    /// The name in the journal and `update.status`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Portable => "portable",
            Self::Unknown => "unknown",
        }
    }
}

/// What [`detect`] found about the running program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installation {
    pub kind: InstallKind,
    /// Why it does not update itself, a code the window translates; `None` for portable.
    pub reason: Option<&'static str>,
    /// The folder the package's entries live in.
    pub install_dir: PathBuf,
    /// The program in it, `/`-separated (`rcommander.exe`, `rCommander.app/Contents/MacOS/rcommander`).
    pub executable: String,
}

/// Every reason [`detect`] gives, for the i18n test.
pub const KIND_REASONS: &[&str] = &[
    "update.kind_marker",
    "update.kind_no_version_file",
    "update.kind_not_writable",
];

/// How the program at `executable` (`std::env::current_exe()`) was installed.
#[must_use]
pub fn detect(executable: &Path) -> Installation {
    let folder = executable.parent().unwrap_or(executable).to_path_buf();
    let name = executable
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // A macOS bundle: `<install>/<Name>.app/Contents/MacOS/<program>`.
    let bundle = folder
        .parent()
        .filter(|contents| contents.file_name().is_some_and(|name| name == "Contents"))
        .and_then(Path::parent)
        .filter(|app| app.extension().is_some_and(|extension| extension == "app"));
    let (install_dir, program, version_files) = match bundle {
        Some(app) if folder.file_name().is_some_and(|name| name == "MacOS") => {
            let app_name = app
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            (
                app.parent().unwrap_or(app).to_path_buf(),
                format!("{app_name}/Contents/MacOS/{name}"),
                vec![
                    folder.join(VERSION_FILE),
                    app.join("Contents").join("Resources").join(VERSION_FILE),
                ],
            )
        }
        _ => (folder.clone(), name, vec![folder.join(VERSION_FILE)]),
    };
    let unknown = |reason| Installation {
        kind: InstallKind::Unknown,
        reason: Some(reason),
        install_dir: install_dir.clone(),
        executable: program.clone(),
    };
    if let Some(kind) = marker(&folder.join(INSTALL_KIND_FILE))
        && kind != "portable"
    {
        return unknown("update.kind_marker");
    }
    if !version_files.iter().any(|path| path.is_file()) {
        return unknown("update.kind_no_version_file");
    }
    if !writable(&install_dir) {
        return unknown("update.kind_not_writable");
    }
    Installation {
        kind: InstallKind::Portable,
        reason: None,
        install_dir,
        executable: program,
    }
}

/// The kind a marker file names, lower-cased; `None` without a readable one.
fn marker(path: &Path) -> Option<String> {
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_MARKER_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_ascii_lowercase)
}

/// Whether this process may create a file in `folder`.
fn writable(folder: &Path) -> bool {
    let probe = folder.join(".update-kind-probe");
    let written = fs::write(&probe, b"probe").is_ok();
    let _ = fs::remove_file(&probe);
    written
}

#[cfg(test)]
#[path = "install_kind_tests.rs"]
mod tests;
