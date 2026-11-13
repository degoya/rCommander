//! Windows: a file's thumbnail as Explorer shows it, through `IShellItemImageFactory::GetImage`
//! with `SIIGBF_THUMBNAILONLY` - a type without a thumbnail handler gives none, not its icon.
//!
//! `unsafe` is allowed in this module for the shell item and its factory: COM calls of the
//! `windows` crate without a safe wrapper. COM is initialised for the call on the calling thread
//! (a blocking thread of the pool) and released after it; the bitmap is owned by a guard that
//! deletes it, and its pixels are read by it (`platform/windows/gdi.rs`). Every call has its
//! `SAFETY:` note.
#![allow(
    unsafe_code,
    reason = "IShellItemImageFactory has no safe wrapper; COM and the bitmap have guards \
              (platform/windows/gdi.rs), every block its SAFETY note"
)]

use std::{os::windows::ffi::OsStrExt, path::Path};

use windows::{
    Win32::{
        Foundation::SIZE,
        System::Com::IBindCtx,
        UI::Shell::{
            IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_BIGGERSIZEOK,
            SIIGBF_THUMBNAILONLY,
        },
    },
    core::PCWSTR,
};

use super::super::decode::{self, Tile};
use crate::platform::gdi::{Bitmap, Com, MemoryDc};

/// The largest bitmap taken from a handler: twice the largest tile.
const MAX_SIDE: u32 = 1024;

/// Explorer's thumbnail of `path`, at most `side` pixels a side.
pub fn thumbnail(path: &Path, side: u32) -> Option<Tile> {
    let _com = Com::init();
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call; no bind context is passed. The
    // interface it returns is reference-counted by the `windows` crate.
    let factory: IShellItemImageFactory =
        unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>) }.ok()?;
    let length = i32::try_from(side).ok()?;
    let size = SIZE {
        cx: length,
        cy: length,
    };
    // A type without a thumbnail handler gives none, not its icon; a larger bitmap is fitted.
    let flags = SIIGBF(SIIGBF_THUMBNAILONLY.0 | SIIGBF_BIGGERSIZEOK.0);
    // SAFETY: `factory` is a valid interface; on success the bitmap belongs to the caller, and
    // `Bitmap` deletes it once.
    let bitmap = unsafe { factory.GetImage(size, flags) }
        .ok()
        .map(|bitmap| Bitmap::owned(bitmap.0))?;
    let (width, height) = fitting(&bitmap)?;
    let dc = MemoryDc::screen()?;
    let bgra = bitmap.pixels(&dc, width, height)?;
    decode::from_bgra(width, height, &bgra, side)
}

/// The bitmap's width and height, within [`MAX_SIDE`].
fn fitting(bitmap: &Bitmap) -> Option<(u32, u32)> {
    let (width, height) = bitmap.dimensions()?;
    let width = u32::try_from(width).ok()?;
    let height = height.unsigned_abs();
    let fits = (1..=MAX_SIDE).contains(&width) && (1..=MAX_SIDE).contains(&height);
    fits.then_some((width, height))
}
