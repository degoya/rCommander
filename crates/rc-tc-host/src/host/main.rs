//! `rc-tc-host <dll> <staging>`: loads one Total Commander plugin DLL and serves the protocol of
//! `rc_tc_host` on stdin and stdout until rCommander closes stdin.
//!
//! rCommander starts it (`rc_tc_host::HostProcess`); it is no program for a person. A DLL that
//! does not load, or a crash inside it, ends this process and nothing else. On Windows the DLL
//! sits behind `TcPlugin` in `windows/`, the crate's only `unsafe`; elsewhere `portable` answers
//! every plugin request with "unsupported".

use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::process::ExitCode;

use rc_tc_host::Server;

#[cfg(not(windows))]
mod portable;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use self::portable as platform;
#[cfg(windows)]
use self::windows as platform;

/// The exit code of a call without its two arguments.
const USAGE: u8 = 2;
/// The exit code of a DLL that does not load.
const NOT_LOADED: u8 = 3;
/// The exit code of a protocol violation by rCommander.
const VIOLATION: u8 = 4;

fn main() -> ExitCode {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [dll, staging] = args.as_slice() else {
        eprintln!("usage: rc-tc-host <plugin dll> <staging folder>");
        return ExitCode::from(USAGE);
    };
    let plugin = match platform::open(dll, staging) {
        Ok(plugin) => plugin,
        Err(reason) => {
            eprintln!("rc-tc-host: the plugin did not load: {reason}");
            return ExitCode::from(NOT_LOADED);
        }
    };
    let mut reader = BufReader::new(io::stdin().lock());
    let mut writer = BufWriter::new(io::stdout().lock());
    match Server::new(plugin, staging.clone()).serve(&mut reader, &mut writer) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rc-tc-host: {error}");
            ExitCode::from(VIOLATION)
        }
    }
}
