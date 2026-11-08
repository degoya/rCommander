//! Starting `rc-tc-host` for one DLL and ending it.
//!
//! The DLL's machine is checked before a process starts. The host gets only its three pipes (no
//! console window, stderr to nothing), an environment cleared to [`ENV_PASSED`] with `TEMP`,
//! `TMP` and `COMMANDER_PATH` pointing into its private staging folder, and the paths each
//! request hands it - no secret, no database, no token. Dropping a [`HostProcess`] kills it.
//!
//! Not in the spike (see the job file): the job object and AppContainer of the sandbox, the
//! request timeout, and the start on first use and end when idle.

use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use rc_process::NoConsoleWindow;

use crate::bridge::{Bridge, BridgeError};
use crate::dll::{HEADER_BYTES, check_machine};

/// The variables passed through from rCommander's environment: what Windows' own DLLs need.
pub const ENV_PASSED: &[&str] = &[
    "SystemRoot",
    "SystemDrive",
    "windir",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
];

/// The variables pointed at the staging folder: temporary files, and where a TC plugin looks for
/// TC's folder and its INI files.
pub const ENV_STAGING: &[&str] = &["TEMP", "TMP", "COMMANDER_PATH"];

/// The host's environment: [`ENV_PASSED`] as `lookup` finds them, [`ENV_STAGING`] at `staging`.
pub fn host_env(
    lookup: impl Fn(&str) -> Option<OsString>,
    staging: &Path,
) -> Vec<(OsString, OsString)> {
    let passed = ENV_PASSED
        .iter()
        .filter_map(|name| lookup(name).map(|value| (OsString::from(name), value)));
    let staged = ENV_STAGING
        .iter()
        .map(|name| (OsString::from(name), staging.as_os_str().to_owned()));
    passed.chain(staged).collect()
}

/// One variable of rCommander's own environment.
fn env_var(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

/// Whether the DLL at `dll` can load into this host, read from its PE header.
pub fn check_dll(dll: &Path) -> Result<(), BridgeError> {
    let mut head = Vec::with_capacity(HEADER_BYTES);
    File::open(dll)
        .and_then(|file| file.take(HEADER_BYTES as u64).read_to_end(&mut head))
        .map_err(|error| BridgeError::Refused(format!("the plugin cannot be read: {error}")))?;
    check_machine(&head).map_err(BridgeError::Refused)
}

/// A running host for one DLL and the bridge to it.
pub struct HostProcess {
    child: Child,
    bridge: Bridge<ChildStdout, ChildStdin>,
}

impl HostProcess {
    /// Starts `host` (the `rc-tc-host` binary) for `dll`, staging in `staging` - a folder only
    /// this host uses, made by the caller, absolute.
    pub fn start(host: &Path, dll: &Path, staging: &Path) -> Result<Self, BridgeError> {
        check_dll(dll)?;
        let mut child = Command::new(host)
            .arg(dll)
            .arg(staging)
            .env_clear()
            .envs(host_env(env_var, staging))
            .current_dir(staging)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .no_console_window()
            .spawn()
            .map_err(BridgeError::Start)?;
        let (Some(stdout), Some(stdin)) = (child.stdout.take(), child.stdin.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BridgeError::Start(std::io::Error::other("no pipes")));
        };
        Ok(Self {
            child,
            bridge: Bridge::new(stdout, stdin),
        })
    }

    /// The bridge; once [`Bridge::is_spent`], the host is killed by dropping this value.
    pub fn bridge(&mut self) -> &mut Bridge<ChildStdout, ChildStdin> {
        &mut self.bridge
    }

    /// Whether the host process still runs.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for HostProcess {
    fn drop(&mut self) {
        // A host that ended on its own has nothing left to kill; the wait reaps it either way.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
