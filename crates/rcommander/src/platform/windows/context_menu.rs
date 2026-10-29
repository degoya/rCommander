//! The Explorer's context menu on rCommander's window (RC-080-02): `IContextMenu` for entries of
//! one folder (`IShellFolder::GetUIObjectOf`) or for the folder itself (`CreateViewObject`, with
//! "New" and "Paste"), rCommander's own entries above the shell's, `TrackPopupMenuEx` at the
//! pointer or at the cursor's row, and `InvokeCommand` for the shell entry chosen.
//!
//! While the menu is open the window's `WM_INITMENUPOPUP` (a submenu such as "Send to" or "Open
//! with" is filled when it opens), `WM_MEASUREITEM`, `WM_DRAWITEM` (entries with icons) and
//! `WM_MENUCHAR` go to the handler's `IContextMenu3` or `IContextMenu2`. The menu sends them to
//! its owner - rCommander's window, whose procedure is winit's - so the window is subclassed
//! (`SetWindowSubclass`) for exactly that time, and every other message passes on untouched.
//!
//! It all runs inside `iced::window::run`, on the event loop's thread that owns the window, as
//! Win32 wants it; the menu is modal, so that thread stays in the menu's loop until it closes.
//! The shell's COM interfaces come from the `windows` crate (typed, reference-counted); the
//! plain Win32 calls - the popup menu, the pointer, COM's start, the subclass - from
//! `windows-sys`, as everywhere else in rCommander (owner's decision, 2026-10-18).

#![allow(
    unsafe_code,
    reason = "the shell's context menu exists only as COM interfaces, Win32 menu calls and a \
              window subclass, all unsafe fns of the windows crates; every block says why it \
              holds"
)]

use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path, ptr};

use iced::{Task, window};
use windows::{
    Win32::{
        Foundation as com,
        System::Com::IBindCtx,
        UI::{
            Shell::{
                CMF_CANRENAME, CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO,
                CMINVOKECOMMANDINFOEX, Common::ITEMIDLIST, GCS_VERBW, IContextMenu, IContextMenu2,
                IContextMenu3, IShellFolder, SEE_MASK_UNICODE, SHBindToObject, SHParseDisplayName,
            },
            WindowsAndMessaging::HMENU as ComMenu,
        },
    },
    core::{Interface, PCSTR, PCWSTR, PSTR},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::ClientToScreen,
    System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    },
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::{DefSubclassProc, ILFree, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetMenuItemCount, HMENU,
            MF_SEPARATOR, MF_STRING, SW_SHOWNORMAL, TPM_RETURNCMD, TPM_RIGHTBUTTON,
            TrackPopupMenuEx, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR,
        },
    },
};

use crate::platform::shell_menu::{Anchor, ShellOutcome, ShellRequest};

/// The menu ids of rCommander's own entries start here; 0 means nothing was chosen.
const OWN_FIRST: u32 = 1;
/// The shell's menu ids, from here to [`SHELL_LAST`]; `InvokeCommand` takes the offset.
const SHELL_FIRST: u32 = 0x100;
const SHELL_LAST: u32 = 0x7fff;
/// The DPI at which a logical pixel is a physical one.
const BASE_DPI: f32 = 96.0;
/// The subclass's id on the window ("RCMU").
const SUBCLASS_ID: usize = 0x5243_4d55;

/// The shell menu for `request` on the app's window.
pub fn task(request: ShellRequest) -> Task<ShellOutcome> {
    crate::native_window::on_window(
        move |window| show(window, &request),
        || ShellOutcome::Failed,
    )
}

/// Builds, shows and answers the menu on the live `window`.
fn show(window: &dyn window::Window, request: &ShellRequest) -> ShellOutcome {
    let Some(handle) = crate::native_window::hwnd_of(window) else {
        return ShellOutcome::Failed;
    };
    let hwnd: HWND = crate::native_window::raw_hwnd(handle);
    let _com = Com::init();
    run(hwnd, request).unwrap_or(ShellOutcome::Failed)
}

fn run(hwnd: HWND, request: &ShellRequest) -> Option<ShellOutcome> {
    let menu = context_menu(hwnd, &request.folder, &request.names)?;
    let popup = Popup::new()?;
    for (index, label) in request.own.iter().enumerate() {
        let label = wide(label);
        let id = OWN_FIRST as usize + index;
        // SAFETY: `popup` is a live menu of ours; the label is NUL-terminated and outlives the
        // call, which copies it.
        succeeded(unsafe { AppendMenuW(popup.0, MF_STRING, id, label.as_ptr()) })?;
    }
    if !request.own.is_empty() {
        // SAFETY: as above; a separator has no text.
        succeeded(unsafe { AppendMenuW(popup.0, MF_SEPARATOR, 0, ptr::null()) })?;
    }
    // SAFETY: `popup` is a live menu of ours.
    let after_own = u32::try_from(unsafe { GetMenuItemCount(popup.0) }).unwrap_or(0);
    // SAFETY: the handler adds its entries to our live menu, with ids in the range given.
    unsafe {
        menu.QueryContextMenu(
            ComMenu(popup.0),
            after_own,
            SHELL_FIRST,
            SHELL_LAST,
            CMF_NORMAL | CMF_CANRENAME,
        )
    }
    .ok()
    .ok()?;
    let at = anchor(hwnd, request.anchor)?;
    let chosen = {
        // Without the forwarding the menu still works; only submenus such as "Send to" stay empty.
        let _forward = Forward::install(hwnd, &menu);
        // SAFETY: `popup` and `hwnd` are live and belong to this thread; with TPM_RETURNCMD the
        // result is the chosen id, 0 for none.
        unsafe {
            TrackPopupMenuEx(
                popup.0,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                at.x,
                at.y,
                hwnd,
                ptr::null(),
            )
        }
    };
    let chosen = u32::try_from(chosen).unwrap_or(0);
    if chosen == 0 {
        return Some(ShellOutcome::Dismissed);
    }
    if chosen < SHELL_FIRST {
        let own = usize::try_from(chosen - OWN_FIRST)
            .ok()
            .filter(|index| *index < request.own.len());
        return Some(own.map_or(ShellOutcome::Dismissed, ShellOutcome::Run));
    }
    let offset = chosen - SHELL_FIRST;
    if verb(&menu, offset).is_some_and(|verb| verb.eq_ignore_ascii_case("rename")) {
        return Some(ShellOutcome::Rename);
    }
    // A handler reports its own failures, and a cancelled confirmation is one; the panel is
    // read again either way.
    let _ = invoke(hwnd, &menu, offset, &request.folder, at);
    Some(ShellOutcome::Invoked)
}

/// The handler of `names` in `folder`, or of `folder` itself when there are none.
fn context_menu(hwnd: HWND, folder: &Path, names: &[String]) -> Option<IContextMenu> {
    let path = wide(folder);
    let mut absolute = Pidl::null();
    // SAFETY: the name is NUL-terminated; the item id list written is ours, freed by `Pidl`.
    unsafe {
        SHParseDisplayName(
            PCWSTR(path.as_ptr()),
            None::<&IBindCtx>,
            &raw mut absolute.0,
            0,
            None,
        )
    }
    .ok()?;
    // SAFETY: `absolute` is a live item id list of a folder, bound from the desktop.
    let shell_folder: IShellFolder =
        unsafe { SHBindToObject(None::<&IShellFolder>, absolute.0, None::<&IBindCtx>) }.ok()?;
    if names.is_empty() {
        // SAFETY: `hwnd` is the live window the menu belongs to.
        return unsafe { shell_folder.CreateViewObject(com::HWND(hwnd)) }.ok();
    }
    let mut children = Vec::with_capacity(names.len());
    for name in names {
        let name = wide(name);
        let mut child = Pidl::null();
        // SAFETY: as above; the id list is relative to `shell_folder`, and the attributes are
        // not asked for (null).
        unsafe {
            shell_folder.ParseDisplayName(
                com::HWND(hwnd),
                None::<&IBindCtx>,
                PCWSTR(name.as_ptr()),
                None,
                &raw mut child.0,
                ptr::null_mut(),
            )
        }
        .ok()?;
        children.push(child);
    }
    let items: Vec<*const ITEMIDLIST> = children.iter().map(|child| child.0.cast_const()).collect();
    // SAFETY: the id lists are live children of `shell_folder` until `children` drops, after
    // the call; the handler keeps copies of its own.
    unsafe { shell_folder.GetUIObjectOf(com::HWND(hwnd), &items, None) }.ok()
}

/// The canonical verb of the shell entry at `offset` ("rename", "delete"), if it has one.
fn verb(menu: &IContextMenu, offset: u32) -> Option<String> {
    let mut buffer = [0_u16; 64];
    // SAFETY: GCS_VERBW writes at most `buffer.len()` UTF-16 units, NUL included, into our
    // buffer; many handlers have no verb and fail, which is `None`.
    unsafe {
        menu.GetCommandString(
            offset as usize,
            GCS_VERBW,
            None,
            PSTR(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
        )
    }
    .ok()?;
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..end]))
}

/// Runs the shell entry at `offset` as the Explorer does: by its offset, in `folder`, at `at`.
fn invoke(
    hwnd: HWND,
    menu: &IContextMenu,
    offset: u32,
    folder: &Path,
    at: POINT,
) -> windows::core::Result<()> {
    let directory = wide(folder);
    let info = CMINVOKECOMMANDINFOEX {
        cbSize: size_of::<CMINVOKECOMMANDINFOEX>() as u32,
        // SEE_MASK_UNICODE is CMIC_MASK_UNICODE: the wide fields are the ones to read.
        fMask: SEE_MASK_UNICODE | CMIC_MASK_PTINVOKE,
        hwnd: com::HWND(hwnd),
        // MAKEINTRESOURCE: the entry's offset in place of a verb's name.
        lpVerb: PCSTR(ptr::without_provenance(offset as usize)),
        lpVerbW: PCWSTR(ptr::without_provenance(offset as usize)),
        lpDirectoryW: PCWSTR(directory.as_ptr()),
        nShow: SW_SHOWNORMAL,
        ptInvoke: com::POINT { x: at.x, y: at.y },
        ..Default::default()
    };
    // SAFETY: `info` is a complete CMINVOKECOMMANDINFOEX, as `cbSize` tells the handler, and its
    // strings outlive the call.
    unsafe { menu.InvokeCommand((&raw const info).cast::<CMINVOKECOMMANDINFO>()) }
}

/// Where the menu opens, in screen pixels.
fn anchor(hwnd: HWND, anchor: Anchor) -> Option<POINT> {
    let mut point = POINT::default();
    match anchor {
        // SAFETY: writes the pointer's position into our `point`.
        Anchor::Pointer => succeeded(unsafe { GetCursorPos(&raw mut point) })?,
        Anchor::Window { x, y } => {
            // SAFETY: `hwnd` is the live window; 0 means it could not tell.
            let dpi = match unsafe { GetDpiForWindow(hwnd) } {
                0 => BASE_DPI,
                dpi => dpi as f32,
            };
            let scale = dpi / BASE_DPI;
            point.x = (x * scale).round() as i32;
            point.y = (y * scale).round() as i32;
            // SAFETY: `hwnd` is the live window; `point` is ours.
            succeeded(unsafe { ClientToScreen(hwnd, &raw mut point) })?;
        }
    }
    Some(point)
}

/// A Win32 `BOOL` as an option: `None` for FALSE.
pub(super) fn succeeded(result: i32) -> Option<()> {
    (result != 0).then_some(())
}

/// `text` as a NUL-terminated UTF-16 string.
pub(super) fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

/// COM on this thread for the menu's (or a drag's, `drag_out.rs`) time; the window's thread has
/// it already (winit's drag and drop), and then the pair of calls only counts.
pub(super) struct Com(bool);

impl Com {
    pub(super) fn init() -> Self {
        // SAFETY: no pointers; a thread that has COM in another mode answers RPC_E_CHANGED_MODE,
        // and then there is nothing to undo.
        let result = unsafe {
            CoInitializeEx(
                ptr::null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
            )
        };
        Self(result >= 0)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: pairs the successful CoInitializeEx of `init`, on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// A popup menu of ours, destroyed with the shell's submenus in it.
struct Popup(HMENU);

impl Popup {
    fn new() -> Option<Self> {
        // SAFETY: no arguments; the menu is ours until `drop`.
        let menu = unsafe { CreatePopupMenu() };
        (!menu.is_null()).then_some(Self(menu))
    }
}

impl Drop for Popup {
    fn drop(&mut self) {
        // SAFETY: the menu is ours and no longer shown; a failure leaks one menu handle.
        let _ = unsafe { DestroyMenu(self.0) };
    }
}

/// An item id list the shell allocated for us.
pub(super) struct Pidl(pub(super) *mut ITEMIDLIST);

impl Pidl {
    pub(super) fn null() -> Self {
        Self(ptr::null_mut())
    }
}

impl Drop for Pidl {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the list came from the shell's allocator and is freed once, here; both
            // crates' ITEMIDLIST are the same C struct.
            unsafe { ILFree(self.0.cast_const().cast()) };
        }
    }
}

/// The handler's interfaces that take menu messages; `IContextMenu3` also takes `WM_MENUCHAR`.
struct Handlers {
    third: Option<IContextMenu3>,
    second: Option<IContextMenu2>,
}

/// The installed subclass; dropping it removes it.
struct Forward {
    hwnd: HWND,
    /// Read by [`forward`] through the subclass's reference data while installed.
    _handlers: Box<Handlers>,
}

impl Forward {
    /// Sends `hwnd`'s menu messages to `menu` until the guard drops; `None` when the handler
    /// takes none or the window cannot be subclassed.
    fn install(hwnd: HWND, menu: &IContextMenu) -> Option<Self> {
        let handlers = Box::new(Handlers {
            third: menu.cast().ok(),
            second: menu.cast().ok(),
        });
        if handlers.third.is_none() && handlers.second.is_none() {
            return None;
        }
        let data = ptr::from_ref::<Handlers>(&*handlers).expose_provenance();
        // SAFETY: `hwnd` is the live window of this thread; `data` points into the box the
        // guard keeps until `drop` has removed the subclass again.
        let installed = unsafe { SetWindowSubclass(hwnd, Some(forward), SUBCLASS_ID, data) };
        succeeded(installed)?;
        Some(Self {
            hwnd,
            _handlers: handlers,
        })
    }
}

impl Drop for Forward {
    fn drop(&mut self) {
        // SAFETY: removes the subclass `install` added, on the same thread, before the handlers
        // it reads are freed; a failure leaves a subclass that only passes messages on.
        let _ = unsafe { RemoveWindowSubclass(self.hwnd, Some(forward), SUBCLASS_ID) };
    }
}

/// The subclass procedure: a menu message to the handler, the rest to the window's own.
unsafe extern "system" fn forward(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    if matches!(
        message,
        WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
    ) {
        // SAFETY: `data` is the address of the handlers the installing `Forward` owns; it
        // removes this subclass before it frees them, and both run on this thread.
        let handlers = unsafe { &*ptr::with_exposed_provenance::<Handlers>(data) };
        let (wide_param, long_param) = (com::WPARAM(wparam), com::LPARAM(lparam));
        if let Some(third) = &handlers.third {
            let mut result = com::LRESULT(0);
            // SAFETY: a menu message of the handler's menu, as IContextMenu3 takes it; `result`
            // is ours.
            let handled = unsafe {
                third.HandleMenuMsg2(message, wide_param, long_param, Some(&raw mut result))
            };
            if handled.is_ok() {
                return result.0;
            }
        } else if let Some(second) = &handlers.second
            && message != WM_MENUCHAR
            // SAFETY: a menu message of the handler's menu, as IContextMenu2 takes it.
            && unsafe { second.HandleMenuMsg(message, wide_param, long_param) }.is_ok()
        {
            // A handled owner-draw message answers TRUE, WM_INITMENUPOPUP 0.
            return isize::from(message != WM_INITMENUPOPUP);
        }
    }
    // SAFETY: the message passes on to the window's own procedure, as a subclass must.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
