//! Dragging files out of rCommander's window (RC-080-01): the shell makes the data object of
//! the files (`IShellItemArray` bound to `BHID_DataObject`: `CF_HDROP`, the shell's id lists and
//! what the Explorer, mail programs and browsers read) and `SHDoDragDrop` runs the drag with the
//! shell's own drop source and drag image. The drop target decides copy or move, Ctrl and Shift
//! as in the Explorer; rCommander offers copy, move and link.
//!
//! The drag runs inside `iced::window::run`, on the event loop's thread that owns the window and
//! where winit started OLE for its own drop target; `SHDoDragDrop` is modal, so that thread
//! stays in the drag's loop until the drop. A drag only starts while the primary mouse button is
//! still down: started without it, OLE would drop at once wherever the pointer stands.
//!
//! The shell's COM interfaces come from the `windows` crate, the plain Win32 calls (the pointer,
//! the buttons, Shift) from `windows-sys`, as in `context_menu.rs`, whose COM, item-id-list and
//! string helpers this module shares.

#![allow(
    unsafe_code,
    reason = "dragging files out exists only as the shell's COM interfaces and Win32 calls on the \
              pointer and the keys, all unsafe fns of the windows crates; every block says why it \
              holds"
)]

use std::path::PathBuf;

use iced::{Task, window};
use windows::{
    Win32::{
        Foundation as com,
        System::{
            Com::{IBindCtx, IDataObject},
            Ole::{DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, IDropSource},
        },
        UI::Shell::{
            BHID_DataObject, Common::ITEMIDLIST, IShellItemArray,
            SHCreateShellItemArrayFromIDLists, SHDoDragDrop, SHParseDisplayName,
        },
    },
    core::PCWSTR,
};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::ScreenToClient,
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON, VK_SHIFT},
        WindowsAndMessaging::{GetClientRect, GetCursorPos, GetSystemMetrics, SM_SWAPBUTTON},
    },
};

use super::context_menu::{Com, Pidl, succeeded, wide};
use crate::{
    native_window,
    platform::drag_out::{DragOutcome, Pointer},
};

/// The drag of `paths` from the app's window.
pub fn task(paths: Vec<PathBuf>) -> Task<DragOutcome> {
    native_window::on_window(move |window| drag(window, &paths), || DragOutcome::Failed)
}

/// The pointer over the app's window and the Shift key.
pub fn pointer() -> Task<Option<Pointer>> {
    native_window::on_window(pointer_in, || None)
}

/// Runs the drag on the live `window`, until the drop or the Escape key.
fn drag(window: &dyn window::Window, paths: &[PathBuf]) -> DragOutcome {
    let Some(handle) = native_window::hwnd_of(window) else {
        return DragOutcome::Failed;
    };
    let hwnd: HWND = native_window::raw_hwnd(handle);
    if !primary_down() {
        return DragOutcome::NotHeld;
    }
    let _com = Com::init();
    let Some(data) = data_object(paths) else {
        return DragOutcome::Failed;
    };
    let offered = DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK;
    // SAFETY: `hwnd` is the live window of this thread and `data` a live data object; without a
    // drop source of ours the shell uses its own. The call returns when the drag is over.
    match unsafe { SHDoDragDrop(Some(com::HWND(hwnd)), &data, None::<&IDropSource>, offered) } {
        Ok(_) => DragOutcome::Ended,
        Err(_) => DragOutcome::Failed,
    }
}

/// The shell's data object of `paths`; `None` when the shell does not know one of them.
fn data_object(paths: &[PathBuf]) -> Option<IDataObject> {
    let mut lists = Vec::with_capacity(paths.len());
    for path in paths {
        let name = wide(path);
        let mut list = Pidl::null();
        // SAFETY: the name is NUL-terminated and outlives the call; the item id list written is
        // ours, freed by `Pidl`.
        unsafe {
            SHParseDisplayName(
                PCWSTR(name.as_ptr()),
                None::<&IBindCtx>,
                &raw mut list.0,
                0,
                None,
            )
        }
        .ok()?;
        lists.push(list);
    }
    let items: Vec<*const ITEMIDLIST> = lists.iter().map(|list| list.0.cast_const()).collect();
    // SAFETY: the id lists are live until `lists` drops, after the call; the array keeps copies
    // of its own.
    let array: IShellItemArray = unsafe { SHCreateShellItemArrayFromIDLists(&items) }.ok()?;
    // SAFETY: asks the live array for its data object, without a bind context.
    unsafe { array.BindToHandler(None::<&IBindCtx>, &BHID_DataObject) }.ok()
}

/// Whether the primary mouse button is down now: the left one, the right one when swapped.
fn primary_down() -> bool {
    // SAFETY: no pointers; non-zero when the buttons are swapped.
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let button = if swapped { VK_RBUTTON } else { VK_LBUTTON };
    key_down(button)
}

/// Whether the key or button `key` is down now: the high bit of its asynchronous state.
fn key_down(key: u16) -> bool {
    // SAFETY: no pointers; any virtual-key code is a valid argument.
    let state = unsafe { GetAsyncKeyState(i32::from(key)) };
    state < 0
}

/// Where the pointer stands across the live `window`, and whether Shift is held.
fn pointer_in(window: &dyn window::Window) -> Option<Pointer> {
    let handle = native_window::hwnd_of(window)?;
    let hwnd: HWND = native_window::raw_hwnd(handle);
    let mut point = POINT::default();
    // SAFETY: writes the pointer's position into our `point`.
    succeeded(unsafe { GetCursorPos(&raw mut point) })?;
    // SAFETY: `hwnd` is the live window of this thread; `point` is ours.
    succeeded(unsafe { ScreenToClient(hwnd, &raw mut point) })?;
    let mut client = RECT::default();
    // SAFETY: as above; `client` is ours.
    succeeded(unsafe { GetClientRect(hwnd, &raw mut client) })?;
    let width = client.right - client.left;
    if width <= 0 {
        return None;
    }
    Some(Pointer {
        across: point.x as f32 / width as f32,
        shift: key_down(VK_SHIFT),
    })
}
