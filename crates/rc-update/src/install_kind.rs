//! How this installation came to be, and therefore whether it updates itself (RC-100-06) or a
//! package manager does (RC-100-52 ... RC-100-54).
//!
//! Only the portable package installs an update itself. Scoop, Homebrew and winget install the
//! same archive a person unpacks by hand, and the deb and rpm packages put the program where only
//! root writes: each of them is updated by its manager, and the window shows the manager's
//! command ([`InstallKind::update_command`]) instead of installing - a program that replaced
//! itself behind the manager's back would leave the manager's records stale. Detection, first
//! match wins:
//!
//! 1. **`RCOMMANDER_INSTALL_KIND`** ([`INSTALL_KIND_ENV`]) names a manager's kind outright - the
//!    escape hatch for a layout nothing below knows. `portable` there skips the steps 2 and 3 and
//!    is still held to step 4; a value that names nothing is ignored.
//! 2. **The executable's path, resolved** where a manager's layout is unmistakable: Scoop's
//!    `<scoop>/apps/rcommander/<version>/rcommander.exe`, Homebrew's `Cellar/rcommander/` (the
//!    Linux formula), winget's `WinGet/Packages/` and `WinGet/Links/`, and a macOS bundle while
//!    Homebrew's `Caskroom/rcommander/` exists (the cask moves the `.app` into `/Applications`, so
//!    only its record in the Caskroom names it). The path wins over the archive's own files.
//! 3. **The marker file** [`INSTALL_KIND_FILE`] (`install-kind`) beside the program, or for a
//!    program in a `bin` folder at `../lib/rcommander/install-kind`: the deb and rpm packages
//!    write `deb`/`rpm` into `/usr/lib/rcommander/`. UTF-8 text, the first line that is neither
//!    empty nor a `#` comment is the kind; at most 1 KiB is read. `portable` goes on to step 4;
//!    a kind this build does not know is [`InstallKind::Unknown`] (`update.kind_marker`).
//! 4. **`VERSION.txt` beside the program** (or in a macOS bundle's `Contents/Resources/`), as
//!    `scripts/package-*.sh` writes it, in a folder this process may write: [`InstallKind::Portable`].
//! 5. Otherwise [`InstallKind::Unknown`] with the reason the window shows: a build from source,
//!    the copy of `scripts/run-windows.sh` (no `VERSION.txt`), or a folder like
//!    `C:\Program Files` that cannot be written. It is offered the download only.
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/install_kind.rs` at commit
//! 8c30caac8 (RD-180-06): its kinds without `msi`, `aur` and `docker`, no container step, the
//! Caskroom rule added for the cask; the reasons and the macOS bundle of the earlier reduced copy
//! (commit 26c8592a6) stay.

use std::fs;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The marker a package build writes beside the program.
pub const INSTALL_KIND_FILE: &str = "install-kind";
/// The environment variable that names the kind outright.
pub const INSTALL_KIND_ENV: &str = "RCOMMANDER_INSTALL_KIND";
/// What every package carries beside the program.
const VERSION_FILE: &str = "VERSION.txt";
/// The most of a marker file that is read.
const MAX_MARKER_BYTES: u64 = 1024;
/// The reason of every kind a package manager updates.
const MANAGER_REASON: &str = "update.kind_package_manager";
/// Where Homebrew lives when `HOMEBREW_PREFIX` is not set: Apple Silicon, Intel, Linux.
const BREW_PREFIXES: &[&str] = &["/opt/homebrew", "/usr/local", "/home/linuxbrew/.linuxbrew"];

/// How this installation was installed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallKind {
    /// The zip or `.tar.gz` package, unpacked by hand: updates itself.
    Portable,
    /// The Scoop bucket's manifest (RC-100-52).
    Scoop,
    /// The tap's cask on macOS, its formula on Linux (RC-100-52).
    Homebrew,
    /// winget's portable zip (RC-100-53).
    Winget,
    /// The Debian/Ubuntu package from the apt repository (RC-100-54).
    Deb,
    /// The Fedora package from the dnf repository (RC-100-54).
    Rpm,
    /// Anything else: offered the download only.
    Unknown,
}

impl InstallKind {
    /// The name in the marker file, the journal, `update.status` and the MCP answer.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Portable => "portable",
            Self::Scoop => "scoop",
            Self::Homebrew => "homebrew",
            Self::Winget => "winget",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::Unknown => "unknown",
        }
    }

    /// The kind `value` names; anything else is `None`, never a guess.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.trim().to_ascii_lowercase().as_str() {
            "portable" => Self::Portable,
            "scoop" => Self::Scoop,
            "homebrew" => Self::Homebrew,
            "winget" => Self::Winget,
            "deb" => Self::Deb,
            "rpm" => Self::Rpm,
            _ => return None,
        })
    }

    /// Whether this kind installs an update itself (RC-100-06): the portable package only.
    #[must_use]
    pub fn installs_itself(self) -> bool {
        self == Self::Portable
    }

    /// Whether this kind's channel carries pre-releases. The tap, the bucket, winget and the apt
    /// and dnf repositories follow the newest plain `vX.Y.Z` only (`release.yml`), so a beta
    /// offered there would come with a command that installs nothing new.
    #[must_use]
    pub fn receives_betas(self) -> bool {
        matches!(self, Self::Portable | Self::Unknown)
    }

    /// The command that updates this installation; `None` where no manager owns it.
    #[must_use]
    pub fn update_command(self) -> Option<&'static str> {
        Some(match self {
            Self::Scoop => "scoop update rcommander",
            Self::Homebrew => "brew upgrade rcommander",
            Self::Winget => "winget upgrade degoya.rCommander",
            Self::Deb => "sudo apt update && sudo apt install --only-upgrade rcommander",
            Self::Rpm => "sudo dnf upgrade --refresh rcommander",
            Self::Portable | Self::Unknown => return None,
        })
    }

    /// The manager's name as the window shows it; product names, not translated.
    #[must_use]
    pub fn manager_name(self) -> Option<&'static str> {
        Some(match self {
            Self::Scoop => "Scoop",
            Self::Homebrew => "Homebrew",
            Self::Winget => "winget",
            Self::Deb => "apt",
            Self::Rpm => "dnf",
            Self::Portable | Self::Unknown => return None,
        })
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
    MANAGER_REASON,
];

/// How the program at `executable` (`std::env::current_exe()`) was installed.
#[must_use]
pub fn detect(executable: &Path) -> Installation {
    let named = std::env::var(INSTALL_KIND_ENV).ok();
    let mut prefixes: Vec<PathBuf> = std::env::var_os("HOMEBREW_PREFIX")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    prefixes.extend(BREW_PREFIXES.iter().map(PathBuf::from));
    detect_with(
        executable,
        &canonical(executable),
        named.as_deref(),
        &prefixes,
    )
}

/// [`detect`] from explicit inputs, so every rule is testable with a fixture: the program as
/// started, its resolved path, the environment's kind and the Homebrew prefixes to look in.
pub(crate) fn detect_with(
    executable: &Path,
    resolved: &Path,
    named: Option<&str>,
    brew_prefixes: &[PathBuf],
) -> Installation {
    let folder = executable.parent().unwrap_or(executable).to_path_buf();
    let name = executable
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bundle = bundle_of(&folder);
    let (install_dir, program, version_files) = match bundle {
        Some(app) => {
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
        None => (folder.clone(), name, vec![folder.join(VERSION_FILE)]),
    };
    let found = |kind: InstallKind, reason: Option<&'static str>| Installation {
        kind,
        reason,
        install_dir: install_dir.clone(),
        executable: program.clone(),
    };
    let managed = |kind: InstallKind| found(kind, Some(MANAGER_REASON));
    let named = named.and_then(InstallKind::parse);
    match named {
        Some(InstallKind::Portable) => {}
        Some(kind) => return managed(kind),
        None => {
            // The cask's `rcommander` command is a link into the bundle.
            let in_bundle = bundle.is_some() || resolved.parent().and_then(bundle_of).is_some();
            let cask = in_bundle && brew_prefixes.iter().any(|prefix| caskroom(prefix));
            if cask {
                return managed(InstallKind::Homebrew);
            }
            if let Some(kind) = kind_from_layout(resolved) {
                return managed(kind);
            }
            if let Some(text) = marker_locations(&folder)
                .iter()
                .find_map(|path| marker(path))
            {
                match InstallKind::parse(&text) {
                    Some(InstallKind::Portable) => {}
                    Some(kind) => return managed(kind),
                    None => {
                        tracing::warn!(value = %text, "the install-kind marker names no known kind");
                        return found(InstallKind::Unknown, Some("update.kind_marker"));
                    }
                }
            }
        }
    }
    if !version_files.iter().any(|path| path.is_file()) {
        return found(InstallKind::Unknown, Some("update.kind_no_version_file"));
    }
    if !writable(&install_dir) {
        return found(InstallKind::Unknown, Some("update.kind_not_writable"));
    }
    found(InstallKind::Portable, None)
}

/// The `.app` of a program in `folder` when it is a macOS bundle's
/// `<install>/<Name>.app/Contents/MacOS/`.
fn bundle_of(folder: &Path) -> Option<&Path> {
    if folder.file_name().is_none_or(|name| name != "MacOS") {
        return None;
    }
    folder
        .parent()
        .filter(|contents| contents.file_name().is_some_and(|name| name == "Contents"))
        .and_then(Path::parent)
        .filter(|app| app.extension().is_some_and(|extension| extension == "app"))
}

/// Whether Homebrew under `prefix` keeps a cask named rcommander.
fn caskroom(prefix: &Path) -> bool {
    prefix.join("Caskroom").join("rcommander").is_dir()
}

/// The canonical path where it resolves, so a link (`/opt/homebrew/bin/rcommander`, scoop's
/// `current`) leads to the layout it points into; the path as given otherwise.
fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path)
        .map(|canonical| strip_verbatim(&canonical))
        .unwrap_or_else(|_| path.to_owned())
}

/// Windows' canonical form is `\\?\C:\...`; the layout checks want the ordinary one.
fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path.to_owned(),
    }
}

/// The package managers whose layout names them.
fn kind_from_layout(executable: &Path) -> Option<InstallKind> {
    let parts: Vec<String> = executable
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    let follows = |first: &str, second: &str| {
        parts
            .windows(2)
            .any(|pair| pair[0] == first && pair[1] == second)
    };
    // Scoop: `<root>\apps\rcommander\<version or current>\rcommander.exe`, with a root that
    // names Scoop (`~\scoop`, `C:\ProgramData\scoop`), so an archive a person unpacked into a
    // folder of their own called `apps` is not taken for it.
    let under_scoop = parts.iter().any(|part| part.contains("scoop"));
    if under_scoop
        && parts.windows(4).any(|window| {
            window[0] == "apps" && window[1] == "rcommander" && window[3] == "rcommander.exe"
        })
    {
        return Some(InstallKind::Scoop);
    }
    // Homebrew's formula: `<prefix>/Cellar/rcommander/<version>/...`.
    if follows("cellar", "rcommander") {
        return Some(InstallKind::Homebrew);
    }
    // winget's portable installs: `%LOCALAPPDATA%\Microsoft\WinGet\Packages\<id>_<source>\`, and
    // the link it may put in `WinGet\Links\`.
    if follows("winget", "packages") || follows("winget", "links") {
        return Some(InstallKind::Winget);
    }
    None
}

/// Where the marker is looked for: beside the program, then, for a program in a `bin` folder,
/// in `../lib/rcommander/`.
fn marker_locations(folder: &Path) -> Vec<PathBuf> {
    let mut locations = vec![folder.join(INSTALL_KIND_FILE)];
    if folder.file_name().is_some_and(|name| name == "bin")
        && let Some(prefix) = folder.parent()
    {
        locations.push(
            prefix
                .join("lib")
                .join("rcommander")
                .join(INSTALL_KIND_FILE),
        );
    }
    locations
}

/// The kind a marker file names, as written; `None` without a readable one.
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
        .map(str::to_owned)
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
