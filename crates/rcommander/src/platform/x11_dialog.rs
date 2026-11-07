//! The X11 half of a dialog's window of its own (RC-080-73): winit sets no `WM_TRANSIENT_FOR`, so
//! a click on the main window raised it over its modal dialog. Before the dialog's window maps it
//! is marked here as transient for its owner and as of the window type dialog; the window manager
//! then keeps it in front of its owner, as Windows keeps an owned window.
//!
//! The marks go over a connection of their own (`x11rb`'s pure-Rust one, no `unsafe`): X11
//! window ids are the server's, so any client may set another's properties, and winit's own
//! connection stays untouched. The round trip at the end makes sure the server has them before
//! winit's connection asks to map the window.

use iced::window::{self, raw_window_handle::RawWindowHandle};
use x11rb::{
    protocol::xproto::{AtomEnum, ConnectionExt as _, PropMode},
    wrapper::ConnectionExt as _,
};

/// The X11 window id of the live `window` that `window::run` hands out; `None` for a window of
/// another back end (Wayland).
pub fn x_window_of(window: &dyn window::Window) -> Option<u32> {
    x_window(window.window_handle().ok()?.as_raw())
}

/// The X11 window id `handle` names: Xlib's (winit's) or XCB's; `None` for any other.
pub fn x_window(handle: RawWindowHandle) -> Option<u32> {
    match handle {
        RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok(),
        RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
        _ => None,
    }
}

/// Marks the X11 window `dialog`, not yet mapped, as the dialog of `owner`: `WM_TRANSIENT_FOR`
/// and `_NET_WM_WINDOW_TYPE_DIALOG`. Blocks for the round trips; off the event loop's thread.
pub fn mark_dialog(dialog: u32, owner: u32) -> anyhow::Result<()> {
    let (connection, _) = x11rb::connect(None)?;
    connection.change_property32(
        PropMode::REPLACE,
        dialog,
        AtomEnum::WM_TRANSIENT_FOR,
        AtomEnum::WINDOW,
        &[owner],
    )?;
    let window_type = connection
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE")?
        .reply()?
        .atom;
    let dialog_type = connection
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE_DIALOG")?
        .reply()?
        .atom;
    connection.change_property32(
        PropMode::REPLACE,
        dialog,
        window_type,
        AtomEnum::ATOM,
        &[dialog_type],
    )?;
    connection.sync()?;
    Ok(())
}

#[cfg(test)]
#[path = "x11_dialog_tests.rs"]
mod tests;
