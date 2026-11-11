//! A device in memory for the tests: objects by id under storages, what a real one answers by
//! id, and switches for what it cannot do, for being unplugged, for counting its listings and
//! for holding an upload before its first byte.

use std::{
    collections::BTreeMap,
    io::Read,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use super::{Abilities, DeviceError, DeviceResult, Object, Session, Sink};

use crate::recover::LockOrRecover as _;

/// Bytes a storage of the fake device has free.
pub(crate) const FREE: u64 = 1 << 30;

#[derive(Clone)]
struct Node {
    parent: Option<String>,
    name: String,
    folder: bool,
    data: Vec<u8>,
}

/// The fake device. Its storages are `s1`, `s2`, ...; every other object is numbered too.
pub(crate) struct FakeDevice {
    nodes: Mutex<BTreeMap<String, Node>>,
    next: AtomicUsize,
    abilities: Abilities,
    /// Answers `Unavailable` to everything while set: unplugged, or locked.
    pub(crate) gone: AtomicBool,
    /// How many times a folder (or the storages) was listed.
    pub(crate) listings: AtomicUsize,
    /// Milliseconds every call waits before it answers: a device that hangs.
    pub(crate) stall: AtomicU64,
    /// Calls waiting out their stall right now, and the most there ever were at once.
    running: AtomicUsize,
    pub(crate) most_at_once: AtomicUsize,
    /// While set, an upload waits before it reads its first byte: a device slower than the copy.
    pub(crate) hold: AtomicBool,
    /// Bytes the uploads have read from their sources so far.
    pub(crate) uploaded: AtomicU64,
    /// Uploads that have ended, kept or not.
    pub(crate) uploads_ended: AtomicUsize,
}

impl FakeDevice {
    /// A device with the storages `storages`, doing what `abilities` says.
    pub(crate) fn new(storages: &[&str], abilities: Abilities) -> Arc<Self> {
        let nodes = storages
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let node = Node {
                    parent: None,
                    name: (*name).to_owned(),
                    folder: true,
                    data: Vec::new(),
                };
                (format!("s{}", index + 1), node)
            })
            .collect();
        Arc::new(Self {
            nodes: Mutex::new(nodes),
            next: AtomicUsize::new(1),
            abilities,
            gone: AtomicBool::new(false),
            listings: AtomicUsize::new(0),
            stall: AtomicU64::new(0),
            running: AtomicUsize::new(0),
            most_at_once: AtomicUsize::new(0),
            hold: AtomicBool::new(false),
            uploaded: AtomicU64::new(0),
            uploads_ended: AtomicUsize::new(0),
        })
    }

    /// How many objects the device holds, storages included.
    pub(crate) fn count(&self) -> usize {
        self.nodes().len()
    }

    fn nodes(&self) -> MutexGuard<'_, BTreeMap<String, Node>> {
        self.nodes.lock_or_recover("fake.nodes")
    }

    fn here(&self) -> DeviceResult<()> {
        let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
        self.most_at_once.fetch_max(now, Ordering::SeqCst);
        let stall = self.stall.load(Ordering::SeqCst);
        if stall > 0 {
            std::thread::sleep(Duration::from_millis(stall));
        }
        self.running.fetch_sub(1, Ordering::SeqCst);
        if self.gone.load(Ordering::SeqCst) {
            return Err(DeviceError::Unavailable);
        }
        Ok(())
    }

    fn object(id: &str, node: &Node) -> Object {
        Object {
            id: id.to_owned(),
            name: node.name.clone(),
            folder: node.folder,
            size: node.data.len() as u64,
            modified: Some(SystemTime::UNIX_EPOCH),
        }
    }

    fn add(&self, parent: &str, name: &str, folder: bool, data: Vec<u8>) -> DeviceResult<Object> {
        let mut nodes = self.nodes();
        if !nodes.get(parent).is_some_and(|node| node.folder) {
            return Err(DeviceError::NotFound);
        }
        let id = format!("o{}", self.next.fetch_add(1, Ordering::SeqCst));
        let node = Node {
            parent: Some(parent.to_owned()),
            name: name.to_owned(),
            folder,
            data,
        };
        let object = Self::object(&id, &node);
        nodes.insert(id, node);
        Ok(object)
    }

    /// What an upload of `size` bytes reads from `source`, block by block as WPD's does: `cancel`
    /// is looked at before each block, a failing read fails it, and so does another size.
    fn receive(
        &self,
        source: &mut dyn Read,
        size: u64,
        cancel: &AtomicBool,
    ) -> DeviceResult<Vec<u8>> {
        self.here()?;
        while self.hold.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut data = Vec::new();
        let mut block = vec![0_u8; 16 * 1024];
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(DeviceError::Cancelled);
            }
            let read = match source.read(&mut block) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(DeviceError::Io(error)),
            };
            data.extend_from_slice(&block[..read]);
            self.uploaded.fetch_add(read as u64, Ordering::SeqCst);
        }
        if data.len() as u64 != size {
            return Err(DeviceError::Io(std::io::Error::other(
                "the upload is not of its stated size",
            )));
        }
        Ok(data)
    }
}

impl Session for FakeDevice {
    fn abilities(&self) -> Abilities {
        self.abilities
    }

    fn storages(&self) -> DeviceResult<Vec<Object>> {
        self.here()?;
        self.listings.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .nodes()
            .iter()
            .filter(|(_, node)| node.parent.is_none())
            .map(|(id, node)| Self::object(id, node))
            .collect())
    }

    fn children(&self, folder: &str) -> DeviceResult<Vec<Object>> {
        self.here()?;
        self.listings.fetch_add(1, Ordering::SeqCst);
        let nodes = self.nodes();
        if !nodes.get(folder).is_some_and(|node| node.folder) {
            return Err(DeviceError::NotFound);
        }
        Ok(nodes
            .iter()
            .filter(|(_, node)| node.parent.as_deref() == Some(folder))
            .map(|(id, node)| Self::object(id, node))
            .collect())
    }

    fn free_space(&self, storage: &str) -> DeviceResult<Option<u64>> {
        self.here()?;
        match self.nodes().get(storage) {
            Some(node) if node.parent.is_none() => Ok(Some(FREE)),
            _ => Err(DeviceError::NotFound),
        }
    }

    fn download(&self, file: &str, mut sink: Sink) -> DeviceResult<()> {
        self.here()?;
        let data = match self.nodes().get(file) {
            Some(node) if !node.folder => node.data.clone(),
            _ => return Err(DeviceError::NotFound),
        };
        // Small chunks, so a reader sees more than one.
        for chunk in data.chunks(7000) {
            if !sink(chunk) {
                break;
            }
        }
        Ok(())
    }

    fn upload(
        &self,
        folder: &str,
        name: &str,
        size: u64,
        mut source: Box<dyn Read + Send>,
        cancel: Arc<AtomicBool>,
    ) -> DeviceResult<Object> {
        let uploaded = self.receive(source.as_mut(), size, &cancel);
        self.uploads_ended.fetch_add(1, Ordering::SeqCst);
        self.add(folder, name, false, uploaded?)
    }

    fn create_folder(&self, parent: &str, name: &str) -> DeviceResult<Object> {
        self.here()?;
        self.add(parent, name, true, Vec::new())
    }

    fn delete(&self, id: &str, recursive: bool) -> DeviceResult<()> {
        self.here()?;
        let mut nodes = self.nodes();
        if !nodes.contains_key(id) {
            return Err(DeviceError::NotFound);
        }
        let has_children = nodes
            .values()
            .any(|node| node.parent.as_deref() == Some(id));
        if has_children && !recursive {
            return Err(DeviceError::Io(std::io::Error::other("folder not empty")));
        }
        let mut doomed = vec![id.to_owned()];
        let mut index = 0;
        while let Some(current) = doomed.get(index).cloned() {
            doomed.extend(
                nodes
                    .iter()
                    .filter(|(_, node)| node.parent.as_deref() == Some(current.as_str()))
                    .map(|(child, _)| child.clone()),
            );
            index += 1;
        }
        for gone in doomed {
            nodes.remove(&gone);
        }
        Ok(())
    }

    fn rename(&self, id: &str, name: &str) -> DeviceResult<()> {
        self.here()?;
        match self.nodes().get_mut(id) {
            Some(node) => {
                name.clone_into(&mut node.name);
                Ok(())
            }
            None => Err(DeviceError::NotFound),
        }
    }
}
