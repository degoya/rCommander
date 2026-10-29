//! The distributions as Windows knows them.
//!
//! The names come from `wsl.exe --list --quiet`: the WSL service is the authority on which
//! distributions exist and are usable, and with WSL switched off it answers with an error, so
//! nothing is listed - while the registry still holds every distribution ever registered.
//!
//! The default and the versions come from the registry (`HKCU\...\Lxss`: `DefaultDistribution`
//! names the default's subkey, each subkey holds `DistributionName` and `Version`). Reading it
//! starts no process and is not translated, unlike `wsl.exe --list --verbose`, whose header and
//! state column follow the display language; that call is the fallback when the registry names
//! no default.
//!
//! The share host follows the Windows build (`CurrentBuildNumber`), not a probe: opening
//! `\\wsl.localhost\<distro>` starts the distribution, and a drive bar must not boot every
//! distribution it shows.

use std::time::Duration;

use rc_process::NoConsoleWindow;
use tokio::process::Command;
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
};

use super::{
    WslDistro, decode,
    details::{Details, LxssEntry, assemble, from_registry, host_for_build},
    query::{capture, names_from},
};

const LXSS: &str = r"Software\Microsoft\Windows\CurrentVersion\Lxss";
const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

/// The installed distributions; none when WSL is missing, switched off, has none or does not
/// answer within `limit`.
pub(super) async fn list(limit: Duration) -> Vec<WslDistro> {
    let names = names_from(wsl(&["--list", "--quiet"]), limit).await;
    if names.is_empty() {
        return Vec::new();
    }
    let details = match registry_details().filter(|details| details.default.is_some()) {
        Some(details) => details,
        None => verbose_details(limit).await,
    };
    assemble(names, &details, host_for_build(windows_build()))
}

/// `wsl.exe` with `args`, without a console window.
fn wsl(args: &[&str]) -> Command {
    let mut command = Command::new("wsl.exe");
    // Versions that know it then write UTF-8; older ones write UTF-16LE, which is read as well.
    command.args(args).env("WSL_UTF8", "1").no_console_window();
    command
}

/// The default and the versions from `wsl.exe --list --verbose`.
async fn verbose_details(limit: Duration) -> Details {
    capture(wsl(&["--list", "--verbose"]), limit)
        .await
        .map(|bytes| decode::verbose(&decode::decode(&bytes)))
        .unwrap_or_default()
}

/// The default and the versions from the registry; `None` without the `Lxss` key.
fn registry_details() -> Option<Details> {
    let lxss = RegKey::predef(HKEY_CURRENT_USER).open_subkey(LXSS).ok()?;
    let default: Option<String> = lxss.get_value("DefaultDistribution").ok();
    let entries: Vec<LxssEntry> = lxss
        .enum_keys()
        .filter_map(Result::ok)
        .filter_map(|guid| {
            let key = lxss.open_subkey(&guid).ok()?;
            let name: String = key.get_value("DistributionName").ok()?;
            let version: Option<u32> = key.get_value("Version").ok();
            Some(LxssEntry {
                guid,
                name,
                version,
            })
        })
        .collect();
    Some(from_registry(default.as_deref(), &entries))
}

/// The Windows build number, e.g. 22631.
fn windows_build() -> Option<u32> {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(CURRENT_VERSION)
        .ok()?;
    let build: String = key.get_value("CurrentBuildNumber").ok()?;
    build.trim().parse().ok()
}
