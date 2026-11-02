//! COM for the WPD calls: the apartment of a worker thread, the objects WPD's classes make, the
//! strings WPD hands out, and the failure an `HRESULT` names.

#![allow(
    unsafe_code,
    reason = "COM and the WPD interfaces exist only as unsafe fns of the windows crate \
              (CoInitializeEx, CoCreateInstance, CoTaskMemFree and the collections' Add); every \
              block says why it holds"
)]

use std::mem::ManuallyDrop;

use windows::{
    Win32::{
        Devices::PortableDevices::{
            IPortableDeviceKeyCollection, IPortableDevicePropVariantCollection,
            IPortableDeviceValues, PortableDeviceKeyCollection,
            PortableDevicePropVariantCollection, PortableDeviceValues,
        },
        Foundation::PROPERTYKEY,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize,
                StructuredStorage::{
                    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
                },
            },
            Variant::VT_LPWSTR,
        },
    },
    core::{GUID, IUnknown, Interface, PWSTR},
};

use super::super::{DeviceError, codes};

/// COM in the multithreaded apartment for a worker thread's life; dropped last of its state, so
/// every interface is released before.
pub(super) struct Com(bool);

impl Com {
    pub(super) fn init() -> Self {
        // SAFETY: no pointers; a thread that has COM in another mode answers RPC_E_CHANGED_MODE,
        // and then there is nothing to undo.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self(result.is_ok())
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

/// The failure a WPD call's error names.
pub(super) fn failed(error: windows::core::Error) -> DeviceError {
    codes::failure(error.code().0.cast_unsigned())
}

/// A new object of the COM class `class`.
pub(super) fn create<T: Interface>(class: &GUID) -> Result<T, DeviceError> {
    // SAFETY: called on a worker thread whose state holds a `Com`; `class` is read for the call
    // only, there is no outer object.
    unsafe { CoCreateInstance(class, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }.map_err(failed)
}

/// An empty value set, what WPD takes its properties in.
pub(super) fn values() -> Result<IPortableDeviceValues, DeviceError> {
    create(&PortableDeviceValues)
}

/// A collection of `keys`, what WPD is asked for properties with.
pub(super) fn keys(keys: &[PROPERTYKEY]) -> Result<IPortableDeviceKeyCollection, DeviceError> {
    let collection: IPortableDeviceKeyCollection = create(&PortableDeviceKeyCollection)?;
    for key in keys {
        // SAFETY: a live collection of this thread; `key` is read for the call, which copies it.
        unsafe { collection.Add(key) }.map_err(failed)?;
    }
    Ok(collection)
}

/// A collection of the one object id `id` (`VT_LPWSTR`), what WPD deletes by.
pub(super) fn ids(id: &str) -> Result<IPortableDevicePropVariantCollection, DeviceError> {
    let collection: IPortableDevicePropVariantCollection =
        create(&PortableDevicePropVariantCollection)?;
    let mut text: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
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
    // the call; `Add` copies the value and takes no ownership of it.
    unsafe { collection.Add(&raw const *variant) }.map_err(failed)?;
    Ok(collection)
}

/// A string a WPD call allocated and handed over, as text; freed here.
pub(super) fn take(text: PWSTR) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: a NUL-terminated string the call allocated with CoTaskMemAlloc and handed to us,
    // read once before it is freed.
    let owned = String::from_utf16_lossy(unsafe { text.as_wide() });
    // SAFETY: freed once, by its owner, after the last read.
    unsafe { CoTaskMemFree(Some(text.as_ptr().cast_const().cast())) };
    owned
}
