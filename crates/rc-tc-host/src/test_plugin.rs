//! A stand-in for a TC DLL, for the tests of the loop and the bridge on every platform.

use std::path::Path;

use crate::rpc::{Entry, Kind};
use crate::server::{Bitmap, PluginError, TcPlugin};

/// Lists `a.txt` and an entry that climbs out of its folder, extracts `a.txt` as `content`,
/// previews a grey square; `crash` makes `list` panic as a DLL's access violation would end the
/// host.
pub(crate) struct Fake {
    pub(crate) content: Vec<u8>,
    pub(crate) crash: bool,
    pub(crate) oversized_preview: bool,
}

impl Fake {
    pub(crate) fn with(content: Vec<u8>) -> Self {
        Self {
            content,
            crash: false,
            oversized_preview: false,
        }
    }
}

pub(crate) fn entry(name: &str, size: u64) -> Entry {
    Entry {
        name: name.to_owned(),
        size,
        packed: size / 2,
        modified: Some(1_709_214_358),
        attributes: 0x20,
    }
}

impl TcPlugin for Fake {
    fn kinds(&self) -> Vec<Kind> {
        vec![Kind::Wcx, Kind::Wlx]
    }

    fn list(&mut self, _archive: &Path) -> Result<Vec<Entry>, PluginError> {
        assert!(!self.crash, "the DLL crashed");
        Ok(vec![
            entry("a.txt", self.content.len() as u64),
            entry("..\\evil.txt", 1),
        ])
    }

    fn extract(&mut self, _archive: &Path, name: &str, target: &Path) -> Result<(), PluginError> {
        if name != "a.txt" {
            return Err(PluginError::Failed("E_NO_FILES".to_owned()));
        }
        std::fs::write(target, &self.content)
            .map_err(|error| PluginError::Failed(error.to_string()))
    }

    fn preview(&mut self, _file: &Path, width: u32, height: u32) -> Result<Bitmap, PluginError> {
        let (width, height) = if self.oversized_preview {
            (width + 1, height)
        } else {
            (width.min(2), height.min(2))
        };
        Ok(Bitmap {
            width,
            height,
            bgra: vec![0x80; (width * height * 4) as usize],
        })
    }
}
