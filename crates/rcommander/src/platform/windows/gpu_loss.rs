//! The message box of `platform/gpu_loss.rs`: `MessageBoxW` through winsafe, without an owner -
//! the app's windows no longer draw - and topmost, so it stands in front of them.

use winsafe::{self as w, co, prelude::*};

/// Yes or No; `false` when the box cannot open.
pub fn ask_restart(title: &str, text: &str) -> bool {
    let flags = co::MB::YESNO | co::MB::ICONWARNING | co::MB::TOPMOST | co::MB::SETFOREGROUND;
    w::HWND::NULL
        .MessageBox(text, title, flags)
        .is_ok_and(|answer| answer == co::DLGID::YES)
}
