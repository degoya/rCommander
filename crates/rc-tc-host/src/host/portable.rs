//! Outside Windows no TC DLL loads: the host still speaks the protocol, reports no kinds and
//! answers every plugin request with "unsupported", so the bridge is tested against the real
//! binary on every platform.

use std::io::Read;
use std::path::Path;

use rc_tc_host::dll::{HEADER_BYTES, check_machine};
use rc_tc_host::rpc::{Entry, Kind};
use rc_tc_host::{Bitmap, PluginError, TcPlugin};

/// The stand-in for a DLL.
pub struct Unsupported;

const REASON: &str = "Total Commander plugins load on Windows only";

/// Checks the file as the Windows host would before loading it, then stands in for it.
pub fn open(dll: &Path, _staging: &Path) -> Result<Unsupported, String> {
    let mut head = Vec::with_capacity(HEADER_BYTES);
    std::fs::File::open(dll)
        .and_then(|file| file.take(HEADER_BYTES as u64).read_to_end(&mut head))
        .map_err(|error| error.to_string())?;
    check_machine(&head)?;
    Ok(Unsupported)
}

impl TcPlugin for Unsupported {
    fn kinds(&self) -> Vec<Kind> {
        Vec::new()
    }

    fn list(&mut self, _archive: &Path) -> Result<Vec<Entry>, PluginError> {
        Err(PluginError::Unsupported(REASON.to_owned()))
    }

    fn extract(
        &mut self,
        _archive: &Path,
        _entry: &str,
        _target: &Path,
    ) -> Result<(), PluginError> {
        Err(PluginError::Unsupported(REASON.to_owned()))
    }

    fn preview(&mut self, _file: &Path, _width: u32, _height: u32) -> Result<Bitmap, PluginError> {
        Err(PluginError::Unsupported(REASON.to_owned()))
    }
}
