//! The Win32 half of the menu bar (RC-080-18): muda's menu on the main window, drawn dark with the
//! app's look (muda's `MenuTheme`, the bar only - the popups stay the system's), shown and hidden
//! without changing the client area's size, and opened with Alt.
//!
//! winit answers `WM_SYSKEYDOWN`, `WM_SYSKEYUP` and `WM_SYSCHAR` itself and never hands them to
//! `DefWindowProc`, which is what opens a window's menu on Alt. So the window is subclassed
//! (`SetWindowSubclass`, beside muda's own subclass for `WM_COMMAND`): the key messages go into
//! [`AltKeys`], and when it says so `WM_SYSCOMMAND`/`SC_KEYMENU` is posted as Windows would send
//! it - winit passes that on to `DefWindowProc`, which runs the menu. A menu by its letter that
//! the bar may not take is stopped at `SC_KEYMENU`, wherever it came from. Every other message
//! goes on to winit unchanged. F10 is not the menu's key here (RC-080-17).
//!
//! The same subclass makes muda's dark bar opaque: muda answers `WM_UAHDRAWMENU` and
//! `WM_UAHDRAWMENUITEM` with GDI's `FillRect` and `DrawTextW`, which leave every pixel's alpha at
//! 0. Where Windows composes the frame with that alpha (the owner's newest Windows 11) the dark
//! bar was see-through, while the light bar, which uxtheme draws, was not. So muda draws into a
//! buffered-paint DIB instead, whose alpha is set to 255 before it is copied onto the window
//! ([`opaque`]). The line muda paints under the bar at `WM_NCPAINT` and `WM_NCACTIVATE`, straight
//! onto the window, is painted over the same way ([`opaque_line`]). `RC_MENU_BAR_THEME`
//! ([`BarPaint`]) turns the parts off one at a time.

#![allow(
    unsafe_code,
    reason = "a window's menu, its client rectangle, its size, the key state and a window \
              subclass exist only as Win32 calls and muda's unsafe hwnd functions; every block \
              says why it holds"
)]

use std::{
    cell::{Cell, RefCell},
    mem, ptr,
};

use iced::window;
use muda::{Menu, MenuTheme};
use windows_sys::Win32::{
    Foundation::{COLORREF, FALSE, HWND, LPARAM, LRESULT, RECT, TRUE, WPARAM},
    Graphics::Gdi::{
        DC_BRUSH, FillRect, GetStockObject, GetWindowDC, HDC, MapWindowPoints, OffsetRect,
        ReleaseDC, SetDCBrushColor,
    },
    UI::{
        Controls::{
            BP_PAINTPARAMS, BPBF_TOPDOWNDIB, BPPF_ERASE, BeginBufferedPaint, BufferedPaintInit,
            BufferedPaintSetAlpha, DRAWITEMSTRUCT, EndBufferedPaint,
        },
        Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GetClientRect, GetMenuBarInfo, GetWindowRect, HMENU, IsIconic, IsZoomed, MENUBARINFO,
            OBJID_MENU, PostMessageW, SC_KEYMENU, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
            SetWindowPos, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_NCACTIVATE,
            WM_NCDESTROY, WM_NCPAINT, WM_RBUTTONDOWN, WM_SYSCHAR, WM_SYSCOMMAND, WM_SYSKEYDOWN,
            WM_SYSKEYUP,
        },
    },
};

use crate::{
    native_window::raw_hwnd as hwnd,
    platform::menu_bar::{AltKeys, BarPaint, Look},
};

/// The subclass's id on the window ("RCMB").
const SUBCLASS_ID: usize = 0x5243_4d42;
/// Bit 30 of a key message's `lParam`: the key was down before - a repeat.
const REPEAT: LPARAM = 1 << 30;
/// The undocumented messages Windows sends to draw a menu bar and one of its items; muda answers
/// them when it draws the bar dark.
const WM_UAHDRAWMENU: u32 = 0x0091;
const WM_UAHDRAWMENUITEM: u32 = 0x0092;
/// muda's dark bar colour (`dark_menu_bar.rs`, `BACKGROUND_COLOR`), which it gives the line under
/// the bar too.
const BAR_BACKGROUND: COLORREF = 0x002B_2B2B;

/// `UAHMENU`, what `WM_UAHDRAWMENU`'s `lParam` points at (the layout muda and win32-darkmode use).
#[repr(C)]
struct UahMenu {
    _hmenu: HMENU,
    hdc: HDC,
    _flags: u32,
}

/// The head of `UAHDRAWMENUITEM`, what `WM_UAHDRAWMENUITEM`'s `lParam` points at; the item's
/// metrics after it are not read.
#[repr(C)]
struct UahDrawMenuItem {
    dis: DRAWITEMSTRUCT,
    um: UahMenu,
}

thread_local! {
    /// What Alt does; read by the subclass on the window's thread.
    static KEYS: RefCell<AltKeys> = RefCell::new(AltKeys::default());
    /// Whether muda's dark drawing goes through the opaque buffer now ([`BarPaint::opaque`]).
    static OPAQUE: Cell<bool> = const { Cell::new(false) };
}

/// Attaches `menu` to the live main `window` with `look` and opens it with Alt and the menus'
/// `letters`; `false` when the window has no handle or muda refuses.
pub fn attach(window: &dyn window::Window, menu: &Menu, look: Look, letters: Vec<char>) -> bool {
    let Some(handle) = crate::native_window::hwnd_of(window) else {
        return false;
    };
    let hwnd = hwnd(handle);
    let paint = BarPaint::get();
    let attached = keep_client(hwnd, || {
        // SAFETY: the live main window `window::run` handed out, on its thread; the caller keeps
        // `menu` for the window's life, so the menu muda set stays valid.
        unsafe { menu.init_for_hwnd_with_theme(handle.get(), theme(paint, look.dark)) }.is_ok()
    });
    if !attached {
        return false;
    }
    OPAQUE.set(paint.opaque(look.dark));
    if !paint.subclass() {
        return true;
    }
    KEYS.with_borrow_mut(|keys| {
        keys.letters = letters;
        keys.active = look.shown && look.enabled;
        keys.letters_free = look.alt_letters;
    });
    // SAFETY: no arguments; buffered painting on this thread, for the process's life. A failure
    // leaves `BeginBufferedPaint` slower, not wrong.
    unsafe { BufferedPaintInit() };
    // SAFETY: the live window of this thread; the procedure reads only this thread's `KEYS` and
    // `OPAQUE` and removes itself when the window is destroyed. A failure leaves the bar without
    // Alt and drawn as muda draws it.
    unsafe { SetWindowSubclass(hwnd, Some(alt_keys), SUBCLASS_ID, 0) };
    true
}

/// Brings `menu`, attached to the live main `window`, from the look `from` to `to`.
pub fn apply(window: &dyn window::Window, menu: &Menu, from: Look, to: Look) {
    let Some(handle) = crate::native_window::hwnd_of(window) else {
        return;
    };
    let hwnd = hwnd(handle);
    if from.shown != to.shown {
        keep_client(hwnd, || {
            // SAFETY: the live main window `menu` is attached to, on its thread.
            let changed = unsafe {
                if to.shown {
                    menu.show_for_hwnd(handle.get())
                } else {
                    menu.hide_for_hwnd(handle.get())
                }
            };
            changed.is_ok()
        });
    }
    if from.dark != to.dark {
        let paint = BarPaint::get();
        OPAQUE.set(paint.opaque(to.dark));
        // SAFETY: as above. A look that cannot be set leaves the bar as it was.
        let _ = unsafe { menu.set_theme_for_hwnd(handle.get(), theme(paint, to.dark)) };
    }
    KEYS.with_borrow_mut(|keys| {
        keys.active = to.shown && to.enabled;
        keys.letters_free = to.alt_letters;
    });
}

/// The bar's look for the app's: muda draws it dark itself (`WM_UAHDRAWMENU`) unless `paint`
/// leaves it to Windows.
fn theme(paint: BarPaint, dark: bool) -> MenuTheme {
    if paint.muda_dark(dark) {
        MenuTheme::Dark
    } else {
        MenuTheme::Light
    }
}

/// Runs `change` - the bar attached, shown or hidden - and gives the window back the client
/// area's height it had, unless it is maximized or minimized: iced stores the client size, and
/// the window would otherwise lose a bar's height at every start.
fn keep_client(hwnd: HWND, change: impl FnOnce() -> bool) -> bool {
    let before = client_height(hwnd);
    let changed = change();
    let after = client_height(hwnd);
    // SAFETY: the live main window, on its thread.
    let resting = unsafe { IsZoomed(hwnd) == FALSE && IsIconic(hwnd) == FALSE };
    if changed
        && resting
        && let (Some(before), Some(after)) = (before, after)
        && before != after
    {
        let mut frame = empty_rect();
        // SAFETY: a live window and a RECT the call writes.
        if unsafe { GetWindowRect(hwnd, &mut frame) } != FALSE {
            // SAFETY: the live main window; only its height changes, not its place, z-order or
            // activation.
            unsafe {
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    0,
                    0,
                    frame.right - frame.left,
                    frame.bottom - frame.top + before - after,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
    changed
}

fn client_height(hwnd: HWND) -> Option<i32> {
    let mut client = empty_rect();
    // SAFETY: a live window and a RECT the call writes.
    (unsafe { GetClientRect(hwnd, &mut client) } != FALSE).then_some(client.bottom - client.top)
}

fn empty_rect() -> RECT {
    RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    }
}

/// The subclass procedure: Alt's messages into [`KEYS`], `SC_KEYMENU` posted when it says so,
/// muda's dark drawing made [`opaque`], and every message on to the window's own procedure.
unsafe extern "system" fn alt_keys(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match message {
        WM_UAHDRAWMENU | WM_UAHDRAWMENUITEM if OPAQUE.get() && lparam != 0 => {
            // SAFETY: Windows' own message for this window, its `lParam` the structure it names.
            return unsafe { opaque(hwnd, message, wparam, lparam) };
        }
        WM_NCPAINT | WM_NCACTIVATE if OPAQUE.get() => {
            // SAFETY: the message passes on unchanged, as a subclass must; muda paints its line
            // in there, which is then painted over.
            let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
            opaque_line(hwnd);
            return result;
        }
        WM_SYSKEYDOWN | WM_KEYDOWN => {
            let alt = wparam == usize::from(VK_MENU);
            let others = held(VK_CONTROL) || held(VK_SHIFT);
            KEYS.with_borrow_mut(|keys| keys.down(alt, others, lparam & REPEAT != 0));
        }
        WM_SYSKEYUP if wparam == usize::from(VK_MENU) => {
            let others = held(VK_CONTROL) || held(VK_SHIFT);
            if KEYS.with_borrow_mut(|keys| keys.alt_up(others)) {
                key_menu(hwnd, 0);
            }
        }
        WM_SYSCHAR => {
            let ctrl = held(VK_CONTROL);
            let letter = u32::try_from(wparam)
                .ok()
                .and_then(char::from_u32)
                .and_then(|typed| KEYS.with_borrow(|keys| keys.letter(typed, ctrl)));
            if let Some(letter) = letter {
                key_menu(hwnd, u32::from(letter) as LPARAM);
            }
        }
        // A menu opened by its letter, also the one Windows opens by itself (owner 2026-10-25:
        // Alt+V in a terminal opened View though the letters were the terminal's): only a
        // letter the bar may take gets through, so Alt+V reaches the shell or Claude Code.
        WM_SYSCOMMAND if wparam & 0xFFF0 == SC_KEYMENU as WPARAM && lparam != 0 => {
            let ctrl = held(VK_CONTROL);
            let allowed = u32::try_from(lparam)
                .ok()
                .and_then(char::from_u32)
                .is_some_and(|typed| KEYS.with_borrow(|keys| keys.letter(typed, ctrl).is_some()));
            if !allowed {
                return 0;
            }
        }
        WM_KILLFOCUS | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            KEYS.with_borrow_mut(AltKeys::disarm);
        }
        WM_NCDESTROY => {
            // SAFETY: the window is being destroyed; a subclass removes itself then.
            unsafe { RemoveWindowSubclass(hwnd, Some(alt_keys), SUBCLASS_ID) };
        }
        _ => {}
    }
    // SAFETY: the message passes on to the window's own procedure, as a subclass must.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// Hands `WM_UAHDRAWMENU`/`WM_UAHDRAWMENUITEM` on to muda with a buffered-paint DC in place of
/// the window's, sets the alpha of everything drawn to 255 and copies it onto the window. Without
/// a buffer muda draws straight onto the window, as before.
///
/// # Safety
///
/// `lparam` is the `UAHMENU` (`WM_UAHDRAWMENU`) or `UAHDRAWMENUITEM` (`WM_UAHDRAWMENUITEM`)
/// Windows passed with `message` to the live window `hwnd`, on its thread.
unsafe fn opaque(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let (menu, rect) = if message == WM_UAHDRAWMENU {
        (
            ptr::with_exposed_provenance_mut::<UahMenu>(lparam as usize),
            bar_rect(hwnd),
        )
    } else {
        let item = ptr::with_exposed_provenance_mut::<UahDrawMenuItem>(lparam as usize);
        // SAFETY: the caller's `UAHDRAWMENUITEM`, valid for this message.
        unsafe { (&raw mut (*item).um, (*item).dis.rcItem) }
    };
    // SAFETY: the caller's structure, valid for this message.
    let target = unsafe { (*menu).hdc };
    let params = erase_params();
    let mut buffered: HDC = ptr::null_mut();
    // SAFETY: the DC Windows draws the bar with, a RECT and parameters that outlive the call, and
    // a DC the call writes.
    let buffer =
        unsafe { BeginBufferedPaint(target, &rect, BPBF_TOPDOWNDIB, &params, &mut buffered) };
    if buffer == 0 {
        // SAFETY: the message passes on unchanged, as a subclass must.
        return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    }
    // SAFETY: the caller's structure: muda draws into the buffer, then Windows gets its DC back.
    // The buffer's DC has the window DC's coordinates, so muda's rectangles hold.
    let result = unsafe {
        (*menu).hdc = buffered;
        let result = DefSubclassProc(hwnd, message, wparam, lparam);
        (*menu).hdc = target;
        result
    };
    // SAFETY: the buffer begun above, ended once; a null RECT is the whole buffer.
    unsafe {
        BufferedPaintSetAlpha(buffer, ptr::null(), 255);
        EndBufferedPaint(buffer, TRUE);
    }
    result
}

/// Paints the one-pixel line between the bar and the client area opaque in muda's colour. muda
/// fills it at `WM_NCPAINT` and `WM_NCACTIVATE` with `FillRect` straight onto the window's DC,
/// alpha 0, and the window behind showed through it (owner 2026-10-27, Windows 11).
fn opaque_line(hwnd: HWND) {
    let mut line = empty_rect();
    let mut frame = empty_rect();
    // SAFETY: the live window and RECTs the calls write; `MapWindowPoints` reads and writes the
    // RECT as its two POINTs. The same rectangle muda computes.
    unsafe {
        GetClientRect(hwnd, &mut line);
        MapWindowPoints(hwnd, ptr::null_mut(), (&raw mut line).cast(), 2);
        GetWindowRect(hwnd, &mut frame);
        OffsetRect(&mut line, -frame.left, -frame.top);
    }
    line.bottom = line.top;
    line.top -= 1;
    // SAFETY: the live window, on its thread; the DC is released below.
    let target = unsafe { GetWindowDC(hwnd) };
    if target.is_null() {
        return;
    }
    let params = erase_params();
    let mut buffered: HDC = ptr::null_mut();
    // SAFETY: the window's DC, a RECT and parameters that outlive the call, and a DC the call
    // writes.
    let buffer =
        unsafe { BeginBufferedPaint(target, &line, BPBF_TOPDOWNDIB, &params, &mut buffered) };
    if buffer != 0 {
        // SAFETY: the buffer begun above and its DC, ended once; the stock DC brush is never
        // deleted. A null RECT is the whole buffer.
        unsafe {
            SetDCBrushColor(buffered, BAR_BACKGROUND);
            FillRect(buffered, &line, GetStockObject(DC_BRUSH));
            BufferedPaintSetAlpha(buffer, ptr::null(), 255);
            EndBufferedPaint(buffer, TRUE);
        }
    }
    // SAFETY: the DC taken above, released once.
    unsafe { ReleaseDC(hwnd, target) };
}

/// Buffered painting's parameters: the buffer starts cleared.
fn erase_params() -> BP_PAINTPARAMS {
    BP_PAINTPARAMS {
        cbSize: mem::size_of::<BP_PAINTPARAMS>() as u32,
        dwFlags: BPPF_ERASE,
        prcExclude: ptr::null(),
        pBlendFunction: ptr::null(),
    }
}

/// The bar's rectangle in window coordinates, a pixel higher, as muda fills it.
fn bar_rect(hwnd: HWND) -> RECT {
    let mut info = MENUBARINFO {
        cbSize: mem::size_of::<MENUBARINFO>() as u32,
        rcBar: empty_rect(),
        hMenu: ptr::null_mut(),
        hwndMenu: ptr::null_mut(),
        _bitfield: 0,
    };
    let mut frame = empty_rect();
    // SAFETY: the live window and structures the calls write; a failure leaves them empty.
    unsafe {
        GetMenuBarInfo(hwnd, OBJID_MENU, 0, &mut info);
        GetWindowRect(hwnd, &mut frame);
        OffsetRect(&mut info.rcBar, -frame.left, -frame.top);
    }
    info.rcBar.top -= 1;
    info.rcBar
}

/// Whether the key `key` is held down now.
fn held(key: u16) -> bool {
    // SAFETY: reads this thread's keyboard state; no pointers.
    unsafe { GetKeyState(i32::from(key)) < 0 }
}

/// Opens the bar as Windows does on Alt: `letter` 0 highlights it, a mnemonic opens that menu.
/// Posted, so the menu's loop starts after this key message is done.
fn key_menu(hwnd: HWND, letter: LPARAM) {
    // SAFETY: the live main window, on its thread; winit hands `WM_SYSCOMMAND` on to
    // `DefWindowProc`, which runs the menu.
    unsafe { PostMessageW(hwnd, WM_SYSCOMMAND, SC_KEYMENU as WPARAM, letter) };
}
