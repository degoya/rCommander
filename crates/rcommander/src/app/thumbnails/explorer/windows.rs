//! Windows: a file's thumbnail as Explorer shows it, through `IShellItemImageFactory::GetImage`
//! with `SIIGBF_THUMBNAILONLY` - a type without a thumbnail handler gives none, not its icon.
//!
//! `unsafe` is allowed in this module, and only here: the shell item and its factory are COM
//! calls of the `windows` crate, the bitmap's pixels GDI's bare `extern` functions of
//! `windows-sys`, none with a safe wrapper. COM is initialised for the call on the calling
//! thread (a blocking thread of the pool) and released after it; the bitmap is owned by a guard
//! that deletes it. Every call has its `SAFETY:` note.
#![allow(
    unsafe_code,
    reason = "IShellItemImageFactory and the GDI bitmap calls have no safe wrapper; COM and the \
              bitmap have guards, every block its SAFETY note"
)]

use std::{
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::{null, null_mut},
};

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
use windows_sys::Win32::{
    Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC,
    },
    System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    },
};

use super::super::decode::{self, Tile};

/// The largest bitmap taken from a handler: twice the largest tile.
const MAX_SIDE: u32 = 1024;

/// Explorer's thumbnail of `path`, at most `side` pixels a side.
pub fn thumbnail(path: &Path, side: u32) -> Option<Tile> {
    let _com = Com::start();
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
        .map(|bitmap| Bitmap(bitmap.0))?;
    let (width, height) = bitmap.size()?;
    let bgra = bitmap.pixels(width, height)?;
    decode::from_bgra(width, height, &bgra, side)
}

/// COM on this thread for one call; released when it ends, if this call started it.
struct Com(bool);

impl Com {
    fn start() -> Self {
        // SAFETY: no pointers; called on the thread that drops the guard. A success (S_OK or
        // S_FALSE) is balanced by `CoUninitialize` in `drop`; a thread that has COM in another
        // mode answers RPC_E_CHANGED_MODE, and then there is nothing to undo.
        let result = unsafe {
            CoInitializeEx(
                null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
            )
        };
        Self(result >= 0)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful `CoInitializeEx` of `start` on this thread; every
            // interface of the call is released before the guard drops (declared after it).
            unsafe { CoUninitialize() };
        }
    }
}

/// A bitmap handle this module owns.
struct Bitmap(HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: GetImage created the bitmap for the caller, who deletes it once; it is
            // selected into no DC.
            unsafe { DeleteObject(self.0) };
        }
    }
}

/// A memory device context this module owns.
struct MemoryDc(HDC);

impl Drop for MemoryDc {
    fn drop(&mut self) {
        // SAFETY: made by CreateCompatibleDC below, deleted once.
        unsafe { DeleteDC(self.0) };
    }
}

impl Bitmap {
    /// The bitmap's width and height, within [`MAX_SIDE`].
    fn size(&self) -> Option<(u32, u32)> {
        if self.0.is_null() {
            return None;
        }
        let mut bitmap = BITMAP::default();
        // SAFETY: `self.0` is a valid bitmap; the buffer is a BITMAP of the size passed.
        let read =
            unsafe { GetObjectW(self.0, size_of::<BITMAP>() as i32, (&raw mut bitmap).cast()) };
        if read == 0 {
            return None;
        }
        let width = u32::try_from(bitmap.bmWidth).ok()?;
        let height = bitmap.bmHeight.unsigned_abs();
        let fits = (1..=MAX_SIDE).contains(&width) && (1..=MAX_SIDE).contains(&height);
        fits.then_some((width, height))
    }

    /// The pixels as 32-bit BGRA rows from the top.
    fn pixels(&self, width: u32, height: u32) -> Option<Vec<u8>> {
        // SAFETY: a null DC asks for a memory DC compatible with the screen.
        let dc = MemoryDc(unsafe { CreateCompatibleDC(null_mut()) });
        if dc.0.is_null() {
            return None;
        }
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                // Negative: top-down rows, whichever way the bitmap keeps them.
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        // SAFETY: `pixels` holds `height` rows of `width` 32-bit pixels, exactly what a 32 bpp
        // BI_RGB request for `height` lines writes (32 bpp rows need no padding); `info`
        // describes that layout; the bitmap is selected into no DC; `dc` is a valid memory DC.
        let lines = unsafe {
            GetDIBits(
                dc.0,
                self.0,
                0,
                height,
                pixels.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            )
        };
        (u32::try_from(lines).ok() == Some(height)).then_some(pixels)
    }
}
