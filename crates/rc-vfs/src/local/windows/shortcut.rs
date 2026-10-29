//! Shell shortcuts (`.lnk`) through `IShellLinkW` and `IPersistFile`, as Explorer and Total
//! Commander make them; `winsafe` wraps the COM calls safely.
//!
//! The COM calls run on a thread of their own (RC-080-51): the caller's is a reused thread of
//! the blocking pool, which another call may have joined to the multi-threaded apartment - an
//! apartment of our own there fails with `RPC_E_CHANGED_MODE`, and `winsafe`'s guard would
//! then end the other call's COM when it drops.

use std::{path::Path, thread};

use winsafe::{self as w, co, prelude::*};

use crate::{Result, VfsError};

/// A shortcut at `real` (the caller's `link`) to `target`, starting in the target's folder.
pub(super) fn create(link: &Path, real: &Path, target: &Path) -> Result<()> {
    let failed = |reason: String| VfsError::Io {
        path: link.to_path_buf(),
        source: std::io::Error::other(reason),
    };
    thread::scope(|scope| {
        thread::Builder::new()
            .name("rc-shortcut".to_owned())
            .spawn_scoped(scope, || create_here(link, real, target))
            .map_err(|error| failed(format!("cannot start the shortcut's thread: {error}")))?
            .join()
            .map_err(|_| failed("creating the shortcut failed".to_owned()))?
    })
}

/// [`create`] on this thread, in an apartment of its own for the call.
fn create_here(link: &Path, real: &Path, target: &Path) -> Result<()> {
    let text = |path: &Path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| VfsError::InvalidPath {
                path: path.to_path_buf(),
                reason: "not valid Unicode",
            })
    };
    let (file, goal) = (text(real)?, text(target)?);
    let failed = |error: co::HRESULT| VfsError::Io {
        path: link.to_path_buf(),
        source: std::io::Error::other(format!("creating the shortcut failed: {error}")),
    };
    // Kept to the end: COM stays initialized on this thread while the objects live.
    let _com = w::CoInitializeEx(co::COINIT::APARTMENTTHREADED | co::COINIT::DISABLE_OLE1DDE)
        .map_err(failed)?;
    let shell_link = w::CoCreateInstance::<w::IShellLink>(
        &co::CLSID::ShellLink,
        None::<&w::IUnknown>,
        co::CLSCTX::INPROC_SERVER,
    )
    .map_err(failed)?;
    shell_link.SetPath(&goal).map_err(failed)?;
    if let Some(folder) = target.parent().and_then(Path::to_str) {
        shell_link.SetWorkingDirectory(folder).map_err(failed)?;
    }
    let persist = shell_link
        .QueryInterface::<w::IPersistFile>()
        .map_err(failed)?;
    persist.Save(Some(&file), true).map_err(failed)
}
