//! The X11 marks themselves need a server; these are the window ids they are set on.

use std::{num::NonZeroU32, ptr::NonNull};

use iced::window::raw_window_handle::{
    RawWindowHandle, WaylandWindowHandle, XcbWindowHandle, XlibWindowHandle,
};

use super::x_window;

#[test]
fn an_xlib_window_is_marked_by_its_id() {
    let handle = RawWindowHandle::Xlib(XlibWindowHandle::new(0x0420_0007));
    assert_eq!(x_window(handle), Some(0x0420_0007));
}

#[test]
fn an_xcb_window_is_marked_by_its_id() {
    let id = NonZeroU32::new(0x0060_0003).expect("not zero");
    let handle = RawWindowHandle::Xcb(XcbWindowHandle::new(id));
    assert_eq!(x_window(handle), Some(0x0060_0003));
}

#[test]
fn a_wayland_window_is_not_marked() {
    let handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(NonNull::dangling()));
    assert_eq!(x_window(handle), None);
}
