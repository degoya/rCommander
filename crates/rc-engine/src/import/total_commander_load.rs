//! Finding and reading the two files of an import from Total Commander (RC-050-09), off the
//! async threads.
//!
//! A path the caller names is read as named; over MCP only a file called `wincmd.ini` or
//! `wcx_ftp.ini` that the read guard lets through, and a refused one answers the same whether it
//! exists or not. The guard refuses `wcx_ftp.ini` itself to every reading tool
//! ([`TC_FTP_PASSWORDS`]); only this reader takes it under that one reason, as it never answers a
//! value - a link of that name to a key is still refused (RC-080-12). A file found by itself
//! (`total_commander_path`) is read when it exists. Either way a file is at most
//! [`super::tc_ini::MAX_FILE_BYTES`] and nothing of it is run.

use std::path::{Path, PathBuf};

use super::{
    tc_ini::{self, Ini, IniError},
    total_commander_path::{
        Registry, WCX_FTP_INI, WINCMD_INI, environment, ftp_candidates, is_named, registry,
        wincmd_candidates,
    },
};
use crate::{
    EngineError,
    read_guard::{ReadGuard, TC_FTP_PASSWORDS},
};

/// Who asks: the window, or an MCP call with its read guard.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Caller {
    pub(crate) mcp: bool,
    pub(crate) guard: Option<ReadGuard>,
}

/// The files read, each with its path.
#[derive(Debug, Default)]
pub(crate) struct Loaded {
    pub(crate) wincmd: Option<(PathBuf, Ini)>,
    pub(crate) ftp: Option<(PathBuf, Ini)>,
    /// `wincmd_not_found`, `ftp_ini_not_found`, `value_too_long`.
    pub(crate) warnings: Vec<String>,
    pub(crate) registry: Registry,
}

impl Loaded {
    /// `%name%` as the import expands it.
    pub(crate) fn lookup(&self, name: &str) -> Option<String> {
        environment(name, self.registry.install_dir.as_deref())
    }
}

/// What to read.
#[derive(Debug, Clone, Default)]
pub(crate) struct Wanted {
    pub(crate) wincmd_ini: Option<String>,
    pub(crate) ftp_ini: Option<String>,
    /// The hotlist or the tabs are asked for.
    pub(crate) wincmd: bool,
    /// The connections are asked for.
    pub(crate) ftp: bool,
}

fn read(path: &Path) -> Result<Ini, EngineError> {
    tc_ini::read_file(path).map_err(|error| match error {
        IniError::TooLarge => EngineError::TcIniTooLarge {
            path: path.display().to_string(),
            max: tc_ini::MAX_FILE_BYTES,
        },
        IniError::NotAFile => EngineError::TcIniUnreadable {
            path: path.display().to_string(),
            source: std::io::Error::other("not a regular file"),
        },
        IniError::Io(source) => EngineError::TcIniUnreadable {
            path: path.display().to_string(),
            source,
        },
    })
}

/// Whether `caller` may read `path` as the file `name`.
fn allowed(caller: Caller, path: &Path, name: &str) -> bool {
    !caller.mcp
        || (is_named(path, name)
            && caller.guard.is_none_or(|guard| {
                guard
                    .refuses(path)
                    .is_none_or(|reason| name == WCX_FTP_INI && reason == TC_FTP_PASSWORDS)
            }))
}

/// The file `named` names: refused, read, or the error why not.
fn named(caller: Caller, named: &str, name: &str) -> Result<(PathBuf, Ini), EngineError> {
    let path = PathBuf::from(named.trim());
    if !allowed(caller, &path, name) {
        return Err(EngineError::TcIniPathRefused {
            path: path.display().to_string(),
        });
    }
    let ini = read(&path)?;
    Ok((path, ini))
}

/// The first of `candidates` that exists and `caller` may read.
fn first_found(
    caller: Caller,
    candidates: Vec<PathBuf>,
    name: &str,
) -> Result<Option<(PathBuf, Ini)>, EngineError> {
    let Some(path) = candidates
        .into_iter()
        .find(|path| path.is_file() && allowed(caller, path, name))
    else {
        return Ok(None);
    };
    let ini = read(&path)?;
    Ok(Some((path, ini)))
}

/// Finds and reads what `wanted` asks for.
///
/// # Errors
///
/// A named file is refused (`tc_ini_path_refused`), cannot be read or is too large; a file found
/// by itself that cannot be read; neither file found (`tc_ini_not_found`).
pub(crate) fn load(wanted: &Wanted, caller: Caller) -> Result<Loaded, EngineError> {
    let mut loaded = Loaded {
        registry: registry(),
        ..Loaded::default()
    };
    let lookup = |name: &str| environment(name, loaded.registry.install_dir.as_deref());
    // wincmd.ini also says where wcx_ftp.ini is.
    let need_wincmd = wanted.wincmd || (wanted.ftp && wanted.ftp_ini.is_none());
    let wincmd = match (&wanted.wincmd_ini, need_wincmd) {
        (Some(path), _) => Some(named(caller, path, WINCMD_INI)?),
        (None, true) => first_found(
            caller,
            wincmd_candidates(&lookup, &loaded.registry),
            WINCMD_INI,
        )?,
        (None, false) => None,
    };
    let mut warnings = Vec::new();
    if need_wincmd && wincmd.is_none() {
        warnings.push("wincmd_not_found".to_owned());
    }
    let ftp = match (&wanted.ftp_ini, wanted.ftp) {
        (Some(path), true) => Some(named(caller, path, WCX_FTP_INI)?),
        (None, true) => {
            let wincmd_path = wincmd.as_ref().map(|(path, _)| path.as_path());
            let configured = wincmd
                .as_ref()
                .and_then(|(_, ini)| ini.get("FTP", "FtpIniName"));
            first_found(
                caller,
                ftp_candidates(wincmd_path, configured, &lookup, &loaded.registry),
                WCX_FTP_INI,
            )?
        }
        (_, false) => None,
    };
    if wanted.ftp && ftp.is_none() {
        warnings.push("ftp_ini_not_found".to_owned());
    }
    if wincmd.is_none() && ftp.is_none() {
        return Err(EngineError::TcIniNotFound);
    }
    let dropped = [&wincmd, &ftp]
        .into_iter()
        .flatten()
        .any(|(_, ini)| ini.dropped > 0);
    if dropped {
        warnings.push("value_too_long".to_owned());
    }
    loaded.wincmd = wincmd;
    loaded.ftp = ftp;
    loaded.warnings = warnings;
    Ok(loaded)
}

#[cfg(test)]
#[path = "total_commander_load_tests.rs"]
mod tests;
