//! The character of `platform/key_label.rs`: `MapVirtualKeyW` through winsafe, which reads the
//! keyboard layout of the calling thread - the window's, since the UI asks while it draws.

use winsafe::{self as w, co};

/// What the active layout prints on the key with the scan code `scan` without modifiers; a dead
/// key's accent as well (the top bit marks it). None when the layout has no key there.
pub fn character(scan: u32) -> Option<char> {
    let virtual_key = w::MapVirtualKey(scan, co::MAPVK::VSC_TO_VK);
    if virtual_key == 0 {
        return None;
    }
    let unit = w::MapVirtualKey(virtual_key, co::MAPVK::VK_TO_CHAR) & 0xFFFF;
    char::from_u32(unit).filter(|c| *c != '\0')
}
