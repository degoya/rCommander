//! The COM and GDI guards the Win32 modules share (RC-110-30): COM on a thread for the time of a
//! call, a bitmap handle that is deleted once, a memory device context, and a bitmap's size and
//! pixels. Before, the shell menu, the shell icons and Explorer's thumbnails each had their own.
//!
//! `unsafe` is allowed in this module: `windows-sys` offers `CoInitializeEx` and the GDI bitmap
//! calls as bare `extern` functions with no safe wrapper. Every handle is owned by a guard that
//! frees it, every call has its `SAFETY:` note.
#![allow(
    unsafe_code,
    reason = "CoInitializeEx and the GDI bitmap calls are bare extern fns of windows-sys; every \
              handle has a guard, every block its SAFETY note"
)]

use std::ptr::{null, null_mut};

use windows_sys::Win32::{
    Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC,
    },
    System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    },
};

/// COM on this thread for as long as the guard lives - a call's time, or the life of a value its
/// thread owns; released when it drops, if this guard started it. A thread that has COM already
/// (the window's, through winit's drag and drop) only counts the pair of calls.
pub(crate) struct Com(bool);

impl Com {
    /// Initialises COM on the calling thread, which is the one that drops the guard.
    pub(crate) fn init() -> Self {
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
            // SAFETY: balances the successful `CoInitializeEx` of `init` on this thread; every
            // interface of the call is released before the guard drops (declared after it).
            unsafe { CoUninitialize() };
        }
    }
}

/// A bitmap handle its holder owns; null for none.
pub(crate) struct Bitmap(HBITMAP);

impl Bitmap {
    /// Takes over `handle`: a bitmap a call created for its caller (`GetIconInfo`,
    /// `IShellItemImageFactory::GetImage`) and that is selected into no device context, or null.
    /// The guard deletes it once.
    pub(crate) fn owned(handle: HBITMAP) -> Self {
        Self(handle)
    }

    /// Whether there is no bitmap.
    pub(crate) fn is_null(&self) -> bool {
        self.0.is_null()
    }

    /// The bitmap's width and height as GDI states them (`bmWidth`, `bmHeight`); `None` for no
    /// bitmap, or one GDI cannot describe.
    pub(crate) fn dimensions(&self) -> Option<(i32, i32)> {
        if self.0.is_null() {
            return None;
        }
        let mut bitmap = BITMAP::default();
        // SAFETY: `self.0` is a valid bitmap; the buffer is a BITMAP of the size passed.
        let read =
            unsafe { GetObjectW(self.0, size_of::<BITMAP>() as i32, (&raw mut bitmap).cast()) };
        (read != 0).then_some((bitmap.bmWidth, bitmap.bmHeight))
    }

    /// The pixels as 32-bit BGRA rows from the top, `height` rows of `width` pixels.
    pub(crate) fn pixels(&self, dc: &MemoryDc, width: u32, height: u32) -> Option<Vec<u8>> {
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

impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the call that made the bitmap created it for the caller (`owned`), who
            // deletes it once; it is selected into no DC.
            unsafe { DeleteObject(self.0) };
        }
    }
}

/// A memory device context its holder owns.
pub(crate) struct MemoryDc(HDC);

impl MemoryDc {
    /// A memory device context compatible with the screen; `None` when Windows gives none.
    pub(crate) fn screen() -> Option<Self> {
        // SAFETY: a null DC asks for a memory DC compatible with the screen.
        let dc = Self(unsafe { CreateCompatibleDC(null_mut()) });
        (!dc.0.is_null()).then_some(dc)
    }
}

impl Drop for MemoryDc {
    fn drop(&mut self) {
        // SAFETY: made by CreateCompatibleDC in `screen`, deleted once.
        unsafe { DeleteDC(self.0) };
    }
}
