//! Windows' own file dialog (RC-010-18): the Common Item Dialog - `IFileOpenDialog`, the
//! `IFileSaveDialog`, a folder with `FOS_PICKFOLDERS` - with rCommander's window as its owner,
//! the field's file types, and the field's folder to start in.
//!
//! The dialog is modal to its owner, but it runs on a thread of its own in a single-threaded COM
//! apartment, as the shell wants it: the event loop keeps turning, so the window keeps drawing
//! (terminals, transfers) while the dialog is open, and the answer comes back over a channel.
//! `winsafe` wraps every COM call safely (`docs/architecture.md`, Windows API crates); the one
//! `unsafe` is the owner's handle, which comes from iced as a number. No overwrite question: the
//! dialog only fills the field, and the action the field is for decides about an existing file.

use std::{
    num::NonZeroIsize,
    path::{Path, PathBuf},
    thread,
};

use iced::{Task, window};
use tokio::sync::oneshot;
use winsafe::{self as w, co, prelude::*};

use crate::platform::file_dialog::{Kind, Request, start};

/// The dialog for `request` over the app's newest window: a dialog's window of its own while one
/// is open (RC-080-15) - over the main window, which that dialog keeps disabled, the file dialog
/// would enable it again when it closes.
pub fn task(request: Request) -> Task<Option<PathBuf>> {
    window::latest()
        .then(|id| match id {
            Some(id) => crate::native_window::hwnd(id),
            None => Task::done(None),
        })
        .then(move |owner| {
            let request = request.clone();
            Task::future(ask(owner, request))
        })
}

/// Shows the dialog on its own thread and waits for its answer.
async fn ask(owner: Option<NonZeroIsize>, request: Request) -> Option<PathBuf> {
    let (send, answer) = oneshot::channel();
    thread::Builder::new()
        .name("rc-file-dialog".to_owned())
        .spawn(move || {
            let _ = send.send(show(owner, &request).ok().flatten());
        })
        .ok()?;
    answer.await.ok().flatten()
}

/// Builds, shows and answers the dialog: the path chosen, `None` when it was cancelled.
fn show(owner: Option<NonZeroIsize>, request: &Request) -> w::HrResult<Option<PathBuf>> {
    // Kept to the end: COM stays initialized on this thread while the dialog lives.
    let _com = w::CoInitializeEx(co::COINIT::APARTMENTTHREADED | co::COINIT::DISABLE_OLE1DDE)?;
    let class = match request.kind {
        Kind::Save => &co::CLSID::FileSaveDialog,
        Kind::Open | Kind::Folder => &co::CLSID::FileOpenDialog,
    };
    // Both classes answer `IFileDialog`, which has every call needed here.
    let dialog = w::CoCreateInstance::<w::IFileDialog>(
        class,
        None::<&w::IUnknown>,
        co::CLSCTX::INPROC_SERVER,
    )?;
    dialog.SetOptions(options(request.kind, dialog.GetOptions()?))?;
    if request.kind != Kind::Folder && !request.filters.is_empty() {
        let types: Vec<(&str, &str)> = request
            .filters
            .iter()
            .map(|filter| (filter.label, filter.spec))
            .collect();
        dialog.SetFileTypes(types.as_slice())?;
    }
    let opening = start(
        request.kind,
        &request.current,
        request.fallback.folder(),
        Path::is_dir,
    );
    let folder = opening.folder.as_deref().and_then(Path::to_str);
    if let Some(item) = folder.and_then(|folder| {
        w::SHCreateItemFromParsingName::<w::IShellItem>(folder, None::<&w::IBindCtx>).ok()
    }) {
        // A folder the dialog refuses only loses the start.
        let _ = dialog.SetFolder(&item);
    }
    if let Some(name) = &opening.name {
        let _ = dialog.SetFileName(name);
    }
    // Cancel is `false`, not an error.
    if !dialog.Show(&owner.map_or(w::HWND::NULL, hwnd))? {
        return Ok(None);
    }
    let path = dialog.GetResult()?.GetDisplayName(co::SIGDN::FILESYSPATH)?;
    Ok(Some(PathBuf::from(path)))
}

/// The dialog's options for `kind` over its `defaults`: only real files and folders (no library,
/// no virtual folder), the process's current directory left alone, the folder must exist.
fn options(kind: Kind, defaults: co::FOS) -> co::FOS {
    let base = (defaults & !co::FOS::OVERWRITEPROMPT)
        | co::FOS::FORCEFILESYSTEM
        | co::FOS::NOCHANGEDIR
        | co::FOS::PATHMUSTEXIST;
    match kind {
        Kind::Open => base | co::FOS::FILEMUSTEXIST,
        Kind::Save => base,
        Kind::Folder => base | co::FOS::PICKFOLDERS,
    }
}

/// The owner's handle as `winsafe` takes it; the colour dialog's too.
#[allow(
    unsafe_code,
    reason = "iced hands out the window's HWND as a number (`native_window::hwnd`), and winsafe \
              builds a handle from a pointer only in an unsafe fn"
)]
pub(super) fn hwnd(handle: NonZeroIsize) -> w::HWND {
    // SAFETY: the number is the live HWND of rCommander's window, which outlives the dialog it
    // owns (the app waits for the answer, and the dialog is modal to it); a window handle is not
    // owned, so the copy closes nothing.
    unsafe { w::HWND::from_ptr(crate::native_window::raw_hwnd(handle)) }
}
