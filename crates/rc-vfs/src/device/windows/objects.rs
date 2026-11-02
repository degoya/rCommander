//! What a WPD device holds: the ids of a folder's objects, and each object's name, kind, size and
//! date - one property request per object, which is what WPD offers without its bulk interface.

#![allow(
    unsafe_code,
    reason = "IPortableDeviceContent, IPortableDeviceProperties and IPortableDeviceValues exist \
              only as unsafe fns of the windows crate; every block says why it holds"
)]

use std::time::SystemTime;

use windows::{
    Win32::{
        Devices::PortableDevices::{
            IPortableDeviceContent, IPortableDeviceKeyCollection, IPortableDeviceProperties,
            IPortableDeviceValues, WPD_CONTENT_TYPE_FOLDER, WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT,
            WPD_FUNCTIONAL_CATEGORY_STORAGE, WPD_FUNCTIONAL_OBJECT_CATEGORY,
            WPD_OBJECT_CONTENT_TYPE, WPD_OBJECT_DATE_MODIFIED, WPD_OBJECT_NAME,
            WPD_OBJECT_ORIGINAL_FILE_NAME, WPD_OBJECT_SIZE, WPD_STORAGE_DESCRIPTION,
        },
        Foundation::{PROPERTYKEY, S_OK},
        System::Variant::VT_DATE,
    },
    core::{HSTRING, PWSTR},
};

use super::com::{failed, take};
use crate::device::{
    DeviceResult, Object,
    codes::{local_time, ole_date},
};

/// The id of the device object, whose children are the storages.
pub(super) const DEVICE: &str = "DEVICE";

/// Ids asked of the enumeration at once.
const BATCH: usize = 64;

/// What is asked of a file or folder.
pub(super) const OBJECT_KEYS: [PROPERTYKEY; 5] = [
    WPD_OBJECT_ORIGINAL_FILE_NAME,
    WPD_OBJECT_NAME,
    WPD_OBJECT_CONTENT_TYPE,
    WPD_OBJECT_SIZE,
    WPD_OBJECT_DATE_MODIFIED,
];

/// What is asked of a child of the device object, to find the storages.
pub(super) const STORAGE_KEYS: [PROPERTYKEY; 3] = [
    WPD_FUNCTIONAL_OBJECT_CATEGORY,
    WPD_STORAGE_DESCRIPTION,
    WPD_OBJECT_NAME,
];

/// The ids of the objects in `parent`.
pub(super) fn child_ids(
    content: &IPortableDeviceContent,
    parent: &str,
) -> DeviceResult<Vec<String>> {
    // SAFETY: live content of this thread; the parent's id lives for the call; no filter.
    let objects =
        unsafe { content.EnumObjects(0, &HSTRING::from(parent), None::<&IPortableDeviceValues>) }
            .map_err(failed)?;
    let mut ids = Vec::new();
    let mut batch = [PWSTR::null(); BATCH];
    loop {
        let mut fetched = 0u32;
        // SAFETY: `batch` has room for its length of ids, the count the call is told; it writes
        // at most that many, says how many in `fetched`, and each string is ours (`take`).
        let result = unsafe { objects.Next(&mut batch, &mut fetched) };
        let count = usize::try_from(fetched).unwrap_or(0).min(BATCH);
        ids.extend(batch[..count].iter().map(|id| take(*id)));
        result.ok().map_err(failed)?;
        // `S_FALSE`: fewer than asked for, the last ones.
        if count == 0 || result != S_OK {
            return Ok(ids);
        }
    }
}

/// The properties `keys` of the object `id`.
fn values_of(
    properties: &IPortableDeviceProperties,
    keys: &IPortableDeviceKeyCollection,
    id: &str,
) -> DeviceResult<IPortableDeviceValues> {
    // SAFETY: live interfaces of this thread; the id lives for the call.
    unsafe { properties.GetValues(&HSTRING::from(id), keys) }.map_err(failed)
}

/// The text `key` of `values`, when it has one that is not empty.
fn text(values: &IPortableDeviceValues, key: &PROPERTYKEY) -> Option<String> {
    // SAFETY: a live value set of this thread; a string it hands out is ours to free (`take`).
    let text = unsafe { values.GetStringValue(key) }.ok().map(take)?;
    (!text.is_empty()).then_some(text)
}

/// The date `key` of `values` (`VT_DATE`, local time), when it has one.
fn date(values: &IPortableDeviceValues, key: &PROPERTYKEY) -> Option<SystemTime> {
    // SAFETY: a live value set of this thread; the variant is ours, cleared by its drop.
    let value = unsafe { values.GetValue(key) }.ok()?;
    if value.vt() != VT_DATE {
        return None;
    }
    // SAFETY: a `VT_DATE` variant holds its date in the `date` field of the union.
    let days = unsafe { value.Anonymous.Anonymous.Anonymous.date };
    local_time(ole_date(days)?)
}

/// The file or folder `id`, read with [`OBJECT_KEYS`] in `keys`. Its name is its file name, else
/// its object name, else its id.
pub(super) fn object(
    properties: &IPortableDeviceProperties,
    keys: &IPortableDeviceKeyCollection,
    id: String,
) -> DeviceResult<Object> {
    let values = values_of(properties, keys, &id)?;
    let name = text(&values, &WPD_OBJECT_ORIGINAL_FILE_NAME)
        .or_else(|| text(&values, &WPD_OBJECT_NAME))
        .unwrap_or_else(|| id.clone());
    // SAFETY: a live value set of this thread; a GUID is copied out.
    let kind = unsafe { values.GetGuidValue(&WPD_OBJECT_CONTENT_TYPE) }.ok();
    let folder = kind.is_some_and(|kind| {
        kind == WPD_CONTENT_TYPE_FOLDER || kind == WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT
    });
    let size = if folder {
        0
    } else {
        // SAFETY: as above; a number is copied out. A file without a size is empty.
        unsafe { values.GetUnsignedLargeIntegerValue(&WPD_OBJECT_SIZE) }.unwrap_or(0)
    };
    Ok(Object {
        id,
        name,
        folder,
        size,
        modified: date(&values, &WPD_OBJECT_DATE_MODIFIED),
    })
}

/// The child `id` of the device object, read with [`STORAGE_KEYS`] in `keys`, when it is a
/// storage: a folder named by its description ("Internal shared storage").
pub(super) fn storage(
    properties: &IPortableDeviceProperties,
    keys: &IPortableDeviceKeyCollection,
    id: String,
) -> DeviceResult<Option<Object>> {
    let values = values_of(properties, keys, &id)?;
    // SAFETY: a live value set of this thread; a GUID is copied out.
    let category = unsafe { values.GetGuidValue(&WPD_FUNCTIONAL_OBJECT_CATEGORY) }.ok();
    if category != Some(WPD_FUNCTIONAL_CATEGORY_STORAGE) {
        return Ok(None);
    }
    let name = text(&values, &WPD_STORAGE_DESCRIPTION)
        .or_else(|| text(&values, &WPD_OBJECT_NAME))
        .unwrap_or_else(|| id.clone());
    Ok(Some(Object {
        id,
        name,
        folder: true,
        size: 0,
        modified: None,
    }))
}
