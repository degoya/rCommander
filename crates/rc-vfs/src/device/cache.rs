//! The objects a [`super::DeviceFs`] found on its way along the names, by path.
//!
//! An MTP device answers by object id, so each path is walked down from the storage, a folder's
//! listing per step. The cache keeps what the walks and the listings found, at most `capacity`
//! paths: the oldest go first, so nothing grows with the size of the device - a folder of ten
//! thousand photos costs the cache its bound, no more. A listing replaces what was kept of the
//! folder's objects, so one deleted on the device itself is gone from the cache too; a change
//! made here forgets the path and everything below it.
//!
//! A folder whose whole listing is kept is *complete* for [`FRESH`]: a name not among its objects
//! is not there, without asking the device again - what a copy of many files into one folder
//! asks for each file. Once one of its objects is pushed out, the folder is no longer complete.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsStr,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::Object;

/// Paths a device's cache keeps at most: a camera folder of that many files stays complete.
pub(crate) const CAPACITY: usize = 16_384;

/// How long a complete folder answers for a name it does not hold; the device may have changed
/// on its own (a photo taken) since.
pub(crate) const FRESH: Duration = Duration::from_secs(30);

/// What the cache says about a name in a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Lookup {
    Found(Object),
    /// The folder is complete and holds no such name.
    Absent,
    /// The device has to be asked.
    Unknown,
}

/// The objects found, by path, the oldest first in line to go.
pub(crate) struct ObjectCache {
    objects: HashMap<PathBuf, Object>,
    /// Every key of `objects` once, in the order it came.
    order: VecDeque<PathBuf>,
    /// The folders whose every object is in `objects`, since when.
    complete: HashMap<PathBuf, Instant>,
    capacity: usize,
}

impl ObjectCache {
    /// An empty cache of at most `capacity` paths (at least one).
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            objects: HashMap::new(),
            order: VecDeque::new(),
            complete: HashMap::new(),
            capacity: capacity.max(1),
        }
    }

    /// The object at `path`, when it is kept.
    #[cfg(test)]
    pub(crate) fn get(&self, path: &Path) -> Option<&Object> {
        self.objects.get(path)
    }

    /// How many paths are kept.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.objects.len()
    }

    /// The object `name` in the folder `dir`: kept under that name, else - in a complete folder
    /// - under the name in another case (a device's names are not case-sensitive), else absent.
    pub(crate) fn lookup(&self, dir: &Path, name: &str, now: Instant) -> Lookup {
        if let Some(object) = self.objects.get(&dir.join(name)) {
            return Lookup::Found(object.clone());
        }
        let fresh = self
            .complete
            .get(dir)
            .is_some_and(|since| now.saturating_duration_since(*since) < FRESH);
        if !fresh {
            return Lookup::Unknown;
        }
        self.objects
            .iter()
            .find(|(path, object)| path.parent() == Some(dir) && same_name(&object.name, name))
            .map_or(Lookup::Absent, |(_, object)| Lookup::Found(object.clone()))
    }

    /// Keeps `object` at `path`; the oldest path goes when the cache is full, and its folder is
    /// no longer complete.
    pub(crate) fn put(&mut self, path: PathBuf, object: Object) {
        if self.objects.insert(path.clone(), object).is_none() {
            self.order.push_back(path);
        }
        while self.objects.len() > self.capacity {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.objects.remove(&oldest);
            if let Some(parent) = oldest.parent() {
                self.complete.remove(parent);
            }
        }
    }

    /// The objects of the folder `dir` as a listing just found them, at `now`: what was kept of
    /// its objects before and is not among them goes (with everything below it), and so does
    /// what was kept below a name that is another object now; then each of `children` is kept,
    /// and the folder is complete while they all are.
    pub(crate) fn replace_children(&mut self, dir: &Path, children: &[Object], now: Instant) {
        let names: HashSet<&str> = children.iter().map(|child| child.name.as_str()).collect();
        let stale: Vec<PathBuf> = self
            .objects
            .keys()
            .filter(|path| path.parent() == Some(dir))
            .filter(|path| {
                path.file_name()
                    .and_then(OsStr::to_str)
                    .is_none_or(|name| !names.contains(name))
            })
            .cloned()
            .collect();
        for path in stale {
            self.forget(&path);
        }
        // Before the objects: pushing one of them out takes it back.
        self.complete.insert(dir.to_path_buf(), now);
        for child in children {
            let path = dir.join(&child.name);
            // Another object under the old name: what was kept below the old one is not its.
            if self
                .objects
                .get(&path)
                .is_some_and(|kept| kept.id != child.id)
            {
                self.forget(&path);
            }
            self.put(path, child.clone());
        }
    }

    /// Forgets `path` and every path below it; the folder holding it stays complete.
    pub(crate) fn forget(&mut self, path: &Path) {
        self.objects.retain(|kept, _| !kept.starts_with(path));
        self.order.retain(|kept| !kept.starts_with(path));
        self.complete.retain(|kept, _| !kept.starts_with(path));
    }
}

/// Whether two names on a device are the same one: equal but for case.
pub(crate) fn same_name(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
