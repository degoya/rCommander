//! The Win32 half of the taskbar's relaunch (RC-100-67): the window's property store
//! (`SHGetPropertyStoreForWindow`) takes the relaunch command, the program's name and the
//! AppUserModelID that `platform/taskbar.rs` built. The relaunch goes in first and the ID last:
//! the taskbar reads a window's relaunch when its ID changes, so this holds for a window already
//! on screen (the main window) as for one not shown yet (a dialog's). The store keeps the values
//! on the window at once; no `Commit`.
//!
//! The properties must be removed before the window is destroyed, or their resources stay with
//! the system (`SHGetPropertyStoreForWindow`'s remarks): [`leave`] sets the three keys to
//! `VT_EMPTY` before a dialog window closes, [`leave_all`] for every window still joined before
//! iced ends. The windows that joined are kept in [`JOINED`] for that.
//!
//! The `windows` crate, not `winsafe`: winsafe's `PropVariant` writes strings only as `VT_BSTR`,
//! and the AppUserModel keys are `VT_LPWSTR` strings.

#![allow(
    unsafe_code,
    reason = "a window's property store exists only as SHGetPropertyStoreForWindow and the COM \
              interface IPropertyStore, unsafe fns of the windows crate; every block says why it \
              holds"
)]

use std::{ffi::c_void, mem::ManuallyDrop, sync::Mutex};

use windows::{
    Win32::{
        Foundation::{HWND, PROPERTYKEY},
        System::{
            Com::StructuredStorage::{
                PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
            },
            Variant::VT_LPWSTR,
        },
        UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
    },
    core::{GUID, PWSTR},
};

use super::context_menu::{Com, wide};
use crate::platform::taskbar::{self, DISPLAY_NAME};
use crate::recover::LockOrRecover as _;

/// The format of the `System.AppUserModel.*` keys (`propkey.h`).
const APP_USER_MODEL: GUID = GUID::from_u128(0x9f4c_2855_9f79_4b39_a8d0_e1d4_2de1_d5f3);
/// `System.AppUserModel.RelaunchCommand`.
const RELAUNCH_COMMAND: PROPERTYKEY = PROPERTYKEY {
    fmtid: APP_USER_MODEL,
    pid: 2,
};
/// `System.AppUserModel.RelaunchDisplayNameResource`.
const RELAUNCH_DISPLAY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: APP_USER_MODEL,
    pid: 4,
};
/// `System.AppUserModel.ID`.
const ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: APP_USER_MODEL,
    pid: 5,
};

/// The windows (their `HWND`s' addresses) that carry the properties and have not left yet.
static JOINED: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// Gives the live `window` (an `HWND`, on the thread that owns it) the program's relaunch and
/// AppUserModelID. A failure leaves the window as Windows made it and a warning in the error log.
pub fn join(window: *mut c_void) {
    let Some(relaunch) = taskbar::current() else {
        tracing::warn!("the taskbar gets no relaunch: the program's path cannot be started");
        return;
    };
    let _com = Com::init();
    let Some(store) = store_of(window) else {
        return;
    };
    // Kept before the first value: whatever of it is set is removed again.
    let mut joined = JOINED.lock_or_recover("taskbar.joined");
    if !joined.contains(&window.expose_provenance()) {
        joined.push(window.expose_provenance());
    }
    drop(joined);
    let values = [
        (&RELAUNCH_COMMAND, relaunch.command.as_str()),
        (&RELAUNCH_DISPLAY_NAME, DISPLAY_NAME),
        (&ID, relaunch.app_id.as_str()),
    ];
    for (key, value) in values {
        if let Err(error) = set(&store, key, value) {
            tracing::warn!(%error, pid = key.pid, "the taskbar's relaunch was not set");
            return;
        }
    }
}

/// Takes the properties off the live `window` before it closes, on the thread that owns it.
pub fn leave(window: *mut c_void) {
    let address = window.expose_provenance();
    let mut joined = JOINED.lock_or_recover("taskbar.joined");
    let Some(index) = joined.iter().position(|&held| held == address) else {
        return;
    };
    joined.swap_remove(index);
    drop(joined);
    clear(window);
}

/// Takes the properties off every window still joined, on the windows' thread, before iced ends
/// and destroys them.
pub fn leave_all() {
    let windows = std::mem::take(&mut *JOINED.lock_or_recover("taskbar.joined"));
    for address in windows {
        clear(std::ptr::with_exposed_provenance_mut(address));
    }
}

/// The three keys of `window` set to `VT_EMPTY`, the ID first.
fn clear(window: *mut c_void) {
    let _com = Com::init();
    let Some(store) = store_of(window) else {
        return;
    };
    for key in [&ID, &RELAUNCH_COMMAND, &RELAUNCH_DISPLAY_NAME] {
        // SAFETY: the live store of a live window and an empty variant (`VT_EMPTY`), which owns
        // nothing; `SetValue` copies it, and its drop (`PropVariantClear`) has nothing to free.
        if let Err(error) = unsafe { store.SetValue(key, &PROPVARIANT::default()) } {
            tracing::warn!(%error, pid = key.pid, "the taskbar's relaunch was not removed");
        }
    }
}

/// The property store of `window`, a live window of this process on the thread that owns it.
fn store_of(window: *mut c_void) -> Option<IPropertyStore> {
    // SAFETY: `window` is a live window of this process, handed out by `window::run` (or kept by
    // `join` and not yet destroyed: iced destroys its windows only after `leave`/`leave_all`) on
    // the thread that owns it; the call only hands out the window's property store.
    match unsafe { SHGetPropertyStoreForWindow(HWND(window)) } {
        Ok(store) => Some(store),
        Err(error) => {
            tracing::warn!(%error, "the window's property store did not open");
            None
        }
    }
}

/// Sets `key` of `store` to the string `value` (`VT_LPWSTR`).
fn set(store: &IPropertyStore, key: &PROPERTYKEY, value: &str) -> windows::core::Result<()> {
    let mut text = wide(value);
    // Never dropped: `PROPVARIANT`'s drop is `PropVariantClear`, which would hand `text`'s buffer
    // to CoTaskMemFree. The variant owns nothing else, so nothing leaks.
    let variant = ManuallyDrop::new(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_LPWSTR,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    pwszVal: PWSTR(text.as_mut_ptr()),
                },
            }),
        },
    });
    // SAFETY: `variant` is a `VT_LPWSTR` whose string is `text`, NUL-terminated and alive past
    // the call. `SetValue` copies the value and takes no ownership of it, so `text`'s own drop is
    // the only one that frees the string.
    unsafe { store.SetValue(key, &*variant) }
}
