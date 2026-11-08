//! Explorer's thumbnails for the local files rCommander does not decode itself - PDFs, videos,
//! documents - behind `thumbnails.explorer` (Windows only, off by default, owner 2026-11-07:
//! the handlers are other programs' code, run inside rCommander on files nobody vouches for).
//! The shell's calls are `windows.rs`, the one module here with `unsafe`; off Windows those files
//! show their icon.

use std::path::Path;

use super::decode::Tile;

#[cfg(windows)]
mod windows;

/// Whether this system has Explorer's thumbnails.
pub const AVAILABLE: bool = cfg!(windows);

/// Explorer's thumbnail of the local file `path`, at most `side` pixels a side; `None` when the
/// shell has none (no handler for the type: the icon stands in). Blocks, and runs the handler:
/// run it off the UI thread, and only with the setting on.
pub fn thumbnail(path: &Path, side: u32) -> Option<Tile> {
    #[cfg(windows)]
    {
        windows::thumbnail(path, side)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, side);
        None
    }
}
