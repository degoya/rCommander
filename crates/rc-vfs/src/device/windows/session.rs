//! One opened WPD device: its COM objects on their worker thread ([`Wpd`]) and the [`Session`]
//! that sends each call there ([`WpdSession`]).

#![allow(
    unsafe_code,
    reason = "IPortableDevice, its content, properties and resources, and the IStream of a \
              transfer exist only as unsafe fns of the windows crate; every block says why it \
              holds"
)]

use std::{
    io::{self, Read},
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};

use windows::{
    Win32::{
        Devices::PortableDevices::{
            IPortableDevice, IPortableDeviceContent, IPortableDeviceDataStream,
            IPortableDeviceKeyCollection, IPortableDeviceProperties, IPortableDeviceResources,
            PORTABLE_DEVICE_DELETE_NO_RECURSION, PORTABLE_DEVICE_DELETE_WITH_RECURSION,
            PortableDeviceFTM, WPD_CLIENT_MAJOR_VERSION, WPD_CLIENT_MINOR_VERSION, WPD_CLIENT_NAME,
            WPD_CLIENT_REVISION,
            WPD_COMMAND_OBJECT_MANAGEMENT_CREATE_OBJECT_WITH_PROPERTIES_AND_DATA,
            WPD_COMMAND_OBJECT_MANAGEMENT_CREATE_OBJECT_WITH_PROPERTIES_ONLY,
            WPD_COMMAND_OBJECT_MANAGEMENT_DELETE_OBJECTS, WPD_COMMAND_OBJECT_PROPERTIES_SET,
            WPD_CONTENT_TYPE_FOLDER, WPD_CONTENT_TYPE_GENERIC_FILE, WPD_OBJECT_CONTENT_TYPE,
            WPD_OBJECT_FORMAT, WPD_OBJECT_FORMAT_PROPERTIES_ONLY, WPD_OBJECT_FORMAT_UNSPECIFIED,
            WPD_OBJECT_NAME, WPD_OBJECT_ORIGINAL_FILE_NAME, WPD_OBJECT_PARENT_ID, WPD_OBJECT_SIZE,
            WPD_RESOURCE_DEFAULT, WPD_STORAGE_FREE_SPACE_IN_BYTES,
        },
        Foundation::PROPERTYKEY,
        System::Com::{IStream, STGC_DEFAULT, STGM_READ},
    },
    core::{HSTRING, Interface, PWSTR, w},
};

use super::{
    com::{Com, create, failed, ids, keys, take, values},
    objects::{DEVICE, OBJECT_KEYS, STORAGE_KEYS, child_ids, object, storage},
};
use crate::device::{Abilities, DeviceError, DeviceResult, Object, Session, Sink, worker::Worker};

/// The bytes moved per call of a transfer: the device's own choice, within these bounds.
const MIN_BUFFER: usize = 64 * 1024;
const MAX_BUFFER: usize = 1024 * 1024;

/// An opened device on its worker thread; the apartment goes last.
pub(super) struct Wpd {
    device: IPortableDevice,
    content: IPortableDeviceContent,
    properties: IPortableDeviceProperties,
    resources: IPortableDeviceResources,
    object_keys: IPortableDeviceKeyCollection,
    storage_keys: IPortableDeviceKeyCollection,
    _com: Com,
}

impl Drop for Wpd {
    fn drop(&mut self) {
        // SAFETY: the live device of this thread, closed once before its interfaces are
        // released; a device that is gone fails the call, which changes nothing.
        let _ = unsafe { self.device.Close() };
    }
}

fn buffer_for(optimal: u32) -> Vec<u8> {
    let size = usize::try_from(optimal)
        .unwrap_or(MAX_BUFFER)
        .clamp(MIN_BUFFER, MAX_BUFFER);
    vec![0; size]
}

/// Writes all of `bytes` into `stream`.
fn write_all(stream: &IStream, mut bytes: &[u8]) -> DeviceResult<()> {
    while !bytes.is_empty() {
        let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let mut written = 0u32;
        // SAFETY: the stream reads at most `length` bytes of `bytes`, which lives for the call,
        // and says how many in `written`.
        unsafe { stream.Write(bytes.as_ptr().cast(), length, Some(&raw mut written)) }
            .ok()
            .map_err(failed)?;
        if written == 0 {
            return Err(DeviceError::Io(io::Error::other(
                "the device took no bytes",
            )));
        }
        bytes = &bytes[usize::try_from(written)
            .unwrap_or(bytes.len())
            .min(bytes.len())..];
    }
    Ok(())
}

/// Sends `source`'s `size` bytes into `stream` and commits it; stops at `cancel`.
fn send(
    stream: &IStream,
    source: &mut dyn Read,
    buffer: &mut [u8],
    size: u64,
    cancel: &AtomicBool,
) -> DeviceResult<()> {
    let mut sent = 0u64;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(DeviceError::Cancelled);
        }
        let read = match source.read(buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(DeviceError::Io(error)),
        };
        write_all(stream, &buffer[..read])?;
        sent = sent.saturating_add(read as u64);
    }
    if sent != size {
        return Err(DeviceError::Io(io::Error::other(
            "the file is not of the size it was announced with",
        )));
    }
    // SAFETY: the live stream of this transfer, committed once after its last byte.
    unsafe { stream.Commit(STGC_DEFAULT) }.map_err(failed)
}

impl Wpd {
    /// Opens the device `id` as rCommander, on this worker thread.
    fn open(id: &str) -> DeviceResult<Self> {
        let com = Com::init();
        let device: IPortableDevice = create(&PortableDeviceFTM)?;
        let client = values()?;
        // SAFETY: live objects of this thread; every key, string and the client set live for
        // their calls, which copy what they keep.
        unsafe {
            client
                .SetStringValue(&WPD_CLIENT_NAME, w!("rCommander"))
                .map_err(failed)?;
            client
                .SetUnsignedIntegerValue(&WPD_CLIENT_MAJOR_VERSION, 1)
                .map_err(failed)?;
            client
                .SetUnsignedIntegerValue(&WPD_CLIENT_MINOR_VERSION, 0)
                .map_err(failed)?;
            client
                .SetUnsignedIntegerValue(&WPD_CLIENT_REVISION, 0)
                .map_err(failed)?;
            device.Open(&HSTRING::from(id), &client).map_err(failed)?;
        }
        // SAFETY: the device opened above, on this thread; each call hands out an interface.
        let content = unsafe { device.Content() }.map_err(failed)?;
        // SAFETY: as above.
        let properties = unsafe { content.Properties() }.map_err(failed)?;
        // SAFETY: as above.
        let resources = unsafe { content.Transfer() }.map_err(failed)?;
        Ok(Self {
            device,
            content,
            properties,
            resources,
            object_keys: keys(&OBJECT_KEYS)?,
            storage_keys: keys(&STORAGE_KEYS)?,
            _com: com,
        })
    }

    /// The commands the device lists; everything when it lists none, so a device that does not
    /// tell is refused by itself, not by us.
    fn abilities(&self) -> Abilities {
        let all = Abilities {
            write: true,
            mkdir: true,
            delete: true,
            rename: true,
        };
        // SAFETY: the live device of this thread; each call hands out an interface.
        let commands = unsafe { self.device.Capabilities() }
            // SAFETY: as above.
            .and_then(|capabilities| unsafe { capabilities.GetSupportedCommands() });
        let Ok(commands) = commands else {
            return all;
        };
        let mut count = 0u32;
        // SAFETY: a live collection of this thread; it writes its count into `count` (the
        // binding takes the pointer as `*const`, the call writes through it).
        if unsafe { commands.GetCount((&raw mut count).cast_const()) }.is_err() || count == 0 {
            return all;
        }
        let mut listed: Vec<PROPERTYKEY> = Vec::new();
        for index in 0..count {
            let mut key = PROPERTYKEY::default();
            // SAFETY: as above; it writes the key at `index` into `key`.
            if unsafe { commands.GetAt(index, (&raw mut key).cast_const()) }.is_ok() {
                listed.push(key);
            }
        }
        let has = |wanted: &PROPERTYKEY| listed.contains(wanted);
        Abilities {
            write: has(&WPD_COMMAND_OBJECT_MANAGEMENT_CREATE_OBJECT_WITH_PROPERTIES_AND_DATA),
            mkdir: has(&WPD_COMMAND_OBJECT_MANAGEMENT_CREATE_OBJECT_WITH_PROPERTIES_ONLY),
            delete: has(&WPD_COMMAND_OBJECT_MANAGEMENT_DELETE_OBJECTS),
            rename: has(&WPD_COMMAND_OBJECT_PROPERTIES_SET),
        }
    }

    fn storages(&self) -> DeviceResult<Vec<Object>> {
        let mut storages = Vec::new();
        for id in child_ids(&self.content, DEVICE)? {
            storages.extend(storage(&self.properties, &self.storage_keys, id)?);
        }
        Ok(storages)
    }

    fn children(&self, folder: &str) -> DeviceResult<Vec<Object>> {
        child_ids(&self.content, folder)?
            .into_iter()
            .map(|id| object(&self.properties, &self.object_keys, id))
            .collect()
    }

    fn free_space(&self, storage: &str) -> DeviceResult<Option<u64>> {
        let wanted = keys(&[WPD_STORAGE_FREE_SPACE_IN_BYTES])?;
        // SAFETY: live interfaces of this thread; the id lives for the call.
        let values = unsafe { self.properties.GetValues(&HSTRING::from(storage), &wanted) }
            .map_err(failed)?;
        // SAFETY: a live value set of this thread; a number is copied out.
        Ok(unsafe { values.GetUnsignedLargeIntegerValue(&WPD_STORAGE_FREE_SPACE_IN_BYTES) }.ok())
    }

    fn download(&self, file: &str, mut sink: Sink) -> DeviceResult<()> {
        let mut optimal = 0u32;
        let mut stream: Option<IStream> = None;
        // SAFETY: live resources of this thread; the id, the key and both out-pointers live for
        // the call.
        unsafe {
            self.resources.GetStream(
                &HSTRING::from(file),
                &WPD_RESOURCE_DEFAULT,
                STGM_READ.0,
                &raw mut optimal,
                &raw mut stream,
            )
        }
        .map_err(failed)?;
        let stream = stream.ok_or(DeviceError::NotFound)?;
        let mut buffer = buffer_for(optimal);
        let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        loop {
            let mut read = 0u32;
            // SAFETY: `buffer` has room for `length` bytes; the stream writes at most that many
            // and says how many in `read`.
            unsafe { stream.Read(buffer.as_mut_ptr().cast(), length, Some(&raw mut read)) }
                .ok()
                .map_err(failed)?;
            let read = usize::try_from(read).unwrap_or(0).min(buffer.len());
            if read == 0 || !sink(&buffer[..read]) {
                return Ok(());
            }
        }
    }

    fn upload(
        &self,
        folder: &str,
        name: &str,
        size: u64,
        mut source: Box<dyn Read + Send>,
        cancel: &AtomicBool,
    ) -> DeviceResult<Object> {
        let properties = values()?;
        // SAFETY: a live value set of this thread; every key and string lives for its call,
        // which copies it.
        unsafe {
            let name = HSTRING::from(name);
            properties
                .SetStringValue(&WPD_OBJECT_PARENT_ID, &HSTRING::from(folder))
                .map_err(failed)?;
            properties
                .SetUnsignedLargeIntegerValue(&WPD_OBJECT_SIZE, size)
                .map_err(failed)?;
            properties
                .SetStringValue(&WPD_OBJECT_ORIGINAL_FILE_NAME, &name)
                .map_err(failed)?;
            properties
                .SetStringValue(&WPD_OBJECT_NAME, &name)
                .map_err(failed)?;
            properties
                .SetGuidValue(&WPD_OBJECT_CONTENT_TYPE, &WPD_CONTENT_TYPE_GENERIC_FILE)
                .map_err(failed)?;
            properties
                .SetGuidValue(&WPD_OBJECT_FORMAT, &WPD_OBJECT_FORMAT_UNSPECIFIED)
                .map_err(failed)?;
        }
        let mut stream: Option<IStream> = None;
        let mut optimal = 0u32;
        // SAFETY: live content of this thread; the properties and both out-pointers live for
        // the call; no cookie is asked for (null is allowed).
        unsafe {
            self.content.CreateObjectWithPropertiesAndData(
                &properties,
                &raw mut stream,
                &raw mut optimal,
                ptr::null_mut(),
            )
        }
        .map_err(failed)?;
        let stream = stream.ok_or_else(|| {
            DeviceError::Io(io::Error::other("the device opened no stream for the file"))
        })?;
        let mut buffer = buffer_for(optimal);
        if let Err(error) = send(&stream, source.as_mut(), &mut buffer, size, cancel) {
            // SAFETY: the uncommitted stream of this transfer: reverted, the device keeps
            // nothing of it.
            let _ = unsafe { stream.Revert() };
            return Err(error);
        }
        let data: IPortableDeviceDataStream = stream.cast().map_err(failed)?;
        // SAFETY: the committed stream of this transfer; the id string is ours (`take`).
        let id = take(unsafe { data.GetObjectID() }.map_err(failed)?);
        Ok(Object {
            id,
            name: name.to_owned(),
            folder: false,
            size,
            modified: Some(SystemTime::now()),
        })
    }

    fn create_folder(&self, parent: &str, name: &str) -> DeviceResult<Object> {
        let properties = values()?;
        // SAFETY: a live value set of this thread; every key and string lives for its call,
        // which copies it.
        unsafe {
            let name = HSTRING::from(name);
            properties
                .SetStringValue(&WPD_OBJECT_PARENT_ID, &HSTRING::from(parent))
                .map_err(failed)?;
            properties
                .SetStringValue(&WPD_OBJECT_NAME, &name)
                .map_err(failed)?;
            properties
                .SetStringValue(&WPD_OBJECT_ORIGINAL_FILE_NAME, &name)
                .map_err(failed)?;
            properties
                .SetGuidValue(&WPD_OBJECT_CONTENT_TYPE, &WPD_CONTENT_TYPE_FOLDER)
                .map_err(failed)?;
            properties
                .SetGuidValue(&WPD_OBJECT_FORMAT, &WPD_OBJECT_FORMAT_PROPERTIES_ONLY)
                .map_err(failed)?;
        }
        let mut id = PWSTR::null();
        // SAFETY: live content of this thread; the properties and the out-pointer live for the
        // call; the id it writes is ours (`take`).
        unsafe {
            self.content
                .CreateObjectWithPropertiesOnly(&properties, &raw mut id)
        }
        .map_err(failed)?;
        Ok(Object {
            id: take(id),
            name: name.to_owned(),
            folder: true,
            size: 0,
            modified: None,
        })
    }

    fn delete(&self, id: &str, recursive: bool) -> DeviceResult<()> {
        let doomed = ids(id)?;
        let options = if recursive {
            PORTABLE_DEVICE_DELETE_WITH_RECURSION
        } else {
            PORTABLE_DEVICE_DELETE_NO_RECURSION
        };
        // SAFETY: live content of this thread; the id collection lives for the call; no
        // per-object results are asked for (null is allowed).
        unsafe {
            self.content
                .Delete(options.0.cast_unsigned(), &doomed, ptr::null_mut())
        }
        .map_err(failed)
    }

    fn rename(&self, id: &str, name: &str) -> DeviceResult<()> {
        let change = values()?;
        // SAFETY: a live value set of this thread; the key and the string live for the call.
        unsafe { change.SetStringValue(&WPD_OBJECT_ORIGINAL_FILE_NAME, &HSTRING::from(name)) }
            .map_err(failed)?;
        // SAFETY: live properties of this thread; the id and the change live for the call.
        let results =
            unsafe { self.properties.SetValues(&HSTRING::from(id), &change) }.map_err(failed)?;
        // SAFETY: a live value set of this thread; the per-property result is copied out.
        match unsafe { results.GetErrorValue(&WPD_OBJECT_ORIGINAL_FILE_NAME) } {
            Ok(code) => code.ok().map_err(failed),
            Err(_) => Ok(()),
        }
    }
}

/// An opened device: its calls run on the thread of its COM objects.
pub(super) struct WpdSession {
    worker: Worker<Wpd>,
    abilities: Abilities,
}

impl WpdSession {
    /// Opens the device `id` on a thread of its own.
    pub(super) fn open(id: &str) -> DeviceResult<Self> {
        let owned = id.to_owned();
        let worker = Worker::start("rc-wpd-device", move || Wpd::open(&owned))?;
        let abilities = worker.run(|wpd| wpd.abilities())?;
        Ok(Self { worker, abilities })
    }
}

impl Session for WpdSession {
    fn abilities(&self) -> Abilities {
        self.abilities
    }

    fn storages(&self) -> DeviceResult<Vec<Object>> {
        self.worker.run(|wpd| wpd.storages())?
    }

    fn children(&self, folder: &str) -> DeviceResult<Vec<Object>> {
        let folder = folder.to_owned();
        self.worker.run(move |wpd| wpd.children(&folder))?
    }

    fn free_space(&self, storage: &str) -> DeviceResult<Option<u64>> {
        let storage = storage.to_owned();
        self.worker.run(move |wpd| wpd.free_space(&storage))?
    }

    fn download(&self, file: &str, sink: Sink) -> DeviceResult<()> {
        let file = file.to_owned();
        self.worker.run(move |wpd| wpd.download(&file, sink))?
    }

    fn upload(
        &self,
        folder: &str,
        name: &str,
        size: u64,
        source: Box<dyn Read + Send>,
        cancel: Arc<AtomicBool>,
    ) -> DeviceResult<Object> {
        let (folder, name) = (folder.to_owned(), name.to_owned());
        self.worker
            .run(move |wpd| wpd.upload(&folder, &name, size, source, &cancel))?
    }

    fn create_folder(&self, parent: &str, name: &str) -> DeviceResult<Object> {
        let (parent, name) = (parent.to_owned(), name.to_owned());
        self.worker
            .run(move |wpd| wpd.create_folder(&parent, &name))?
    }

    fn delete(&self, id: &str, recursive: bool) -> DeviceResult<()> {
        let id = id.to_owned();
        self.worker.run(move |wpd| wpd.delete(&id, recursive))?
    }

    fn rename(&self, id: &str, name: &str) -> DeviceResult<()> {
        let (id, name) = (id.to_owned(), name.to_owned());
        self.worker.run(move |wpd| wpd.rename(&id, &name))?
    }
}
