//! The WPD device manager on its worker thread: which devices are there, and their names.

#![allow(
    unsafe_code,
    reason = "IPortableDeviceManager exists only as unsafe fns of the windows crate; every block \
              says why it holds"
)]

use std::ptr;

use windows::{
    Win32::Devices::PortableDevices::{IPortableDeviceManager, PortableDeviceManager},
    core::{HSTRING, PWSTR},
};

use super::com::{Com, create, take};
use crate::device::{DeviceResult, codes::is_mass_storage};

/// The manager and the apartment it lives in; the apartment goes last.
pub(super) struct Manager {
    manager: IPortableDeviceManager,
    _com: Com,
}

impl Manager {
    /// The manager of this worker thread.
    pub(super) fn open() -> DeviceResult<Self> {
        let com = Com::init();
        let manager = create(&PortableDeviceManager)?;
        Ok(Self { manager, _com: com })
    }

    /// The devices there now, by id and name: the list asked again, the drives with a letter of
    /// their own left out. A device whose name cannot be read is named by its id.
    pub(super) fn devices(&self) -> Vec<(String, String)> {
        // SAFETY: a live manager of this thread; no arguments. A failure keeps the last list.
        let _ = unsafe { self.manager.RefreshDeviceList() };
        let mut count = 0u32;
        // SAFETY: with no array the call only writes the count of devices into `count`.
        if unsafe { self.manager.GetDevices(ptr::null_mut(), &mut count) }.is_err() {
            return Vec::new();
        }
        let mut ids = vec![PWSTR::null(); usize::try_from(count).unwrap_or(0)];
        // SAFETY: `ids` has room for `count` ids, which the call is told; it writes at most that
        // many, says how many in `count`, and each string it writes is ours to free (`take`).
        if unsafe { self.manager.GetDevices(ids.as_mut_ptr(), &mut count) }.is_err() {
            return Vec::new();
        }
        ids.truncate(usize::try_from(count).unwrap_or(0));
        ids.into_iter()
            .map(take)
            .filter(|id| !id.is_empty() && !is_mass_storage(id))
            .map(|id| {
                let name = self.friendly_name(&id).unwrap_or_else(|| id.clone());
                (id, name)
            })
            .collect()
    }

    /// The name the device `id` gives itself ("Pixel 8").
    fn friendly_name(&self, id: &str) -> Option<String> {
        let id = HSTRING::from(id);
        let mut length = 0u32;
        // SAFETY: with no buffer the call only writes the length it needs (in characters, the
        // NUL included) into `length`; an answer that it wants a buffer is no failure here.
        let _ = unsafe {
            self.manager
                .GetDeviceFriendlyName(&id, PWSTR::null(), &mut length)
        };
        let mut buffer = vec![0u16; usize::try_from(length).ok()?];
        if buffer.is_empty() {
            return None;
        }
        // SAFETY: `buffer` has room for `length` characters, the length the call is told; it
        // writes at most that many.
        unsafe {
            self.manager
                .GetDeviceFriendlyName(&id, PWSTR(buffer.as_mut_ptr()), &mut length)
        }
        .ok()?;
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        let name = String::from_utf16_lossy(&buffer[..end]);
        (!name.trim().is_empty()).then_some(name)
    }
}
