//! [`DeviceFs`]: a portable device's objects as a [`Vfs`] whose paths are names.
//!
//! `/` is the device, its folders the storages ("Internal shared storage", "SD card"), below them
//! the device's folders and files. A path is resolved name by name from the storage, through the
//! bounded cache of what was found (`cache.rs`) and the folders' listings where the cache does
//! not know; names match in any case, as a phone's storage does. Every call of the device runs on
//! a blocking thread, held to a time limit (a listing to the listing limit), so a device that
//! hangs never holds its caller. The calls take turns (RC-080-70): a call that ran out of time
//! still runs on its thread until the device answers, and the next one - a retry of a delete -
//! starts only after it, never beside it; one that waits past its own limit fails as timed out
//! without having started.
//!
//! What MTP has not, this backend refuses before the device is asked: appending, setting times
//! or attributes, moving between folders, creating or removing a storage, files in the device's
//! root - and whatever the device says it does not do ([`super::Abilities`]): writing,
//! creating folders, deleting, renaming. A file is written through a temporary file and handed to
//! the device whole (`write.rs`).

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant, SystemTime},
};

use async_trait::async_trait;

use super::{
    DeviceResult, Object, Session,
    cache::{CAPACITY, Lookup, ObjectCache, same_name},
    names::{entry, find, names, path_of, unsupported},
    pipe,
    write::{self, Target},
};
use crate::recover::LockOrRecover as _;
use crate::recover::LogPanic as _;
use crate::{
    Capabilities, Entry, ReadStream, Result, SetAttributes, Timeouts, Vfs, VfsError, WriteMode,
    WriteStream,
};

/// A portable device as a file system.
pub struct DeviceFs {
    session: Arc<dyn Session>,
    cache: Arc<Mutex<ObjectCache>>,
    timeouts: Timeouts,
    /// Held by the call on the device's blocking thread until it returns.
    turn: Arc<tokio::sync::Mutex<()>>,
}

/// What a path names: the device itself, or one of its objects.
enum Node {
    Root,
    Object(Object),
}

impl Node {
    fn is_folder(&self) -> bool {
        match self {
            Self::Root => true,
            Self::Object(object) => object.folder,
        }
    }
}

impl DeviceFs {
    /// The device whose calls `session` makes, each held to the default time limits.
    pub(crate) fn new(session: Arc<dyn Session>) -> Self {
        Self::with_limits(session, CAPACITY, Timeouts::default())
    }

    /// [`DeviceFs::new`] with a cache of `capacity` paths and the limits `timeouts`.
    pub(crate) fn with_limits(
        session: Arc<dyn Session>,
        capacity: usize,
        timeouts: Timeouts,
    ) -> Self {
        Self {
            session,
            cache: Arc::new(Mutex::new(ObjectCache::new(capacity))),
            timeouts,
            turn: Arc::default(),
        }
    }

    fn cache(&self) -> MutexGuard<'_, ObjectCache> {
        self.cache.lock_or_recover("fs.cache")
    }

    /// `work` on a blocking thread once the call before it has returned, given up after `limit`
    /// (the wait for the turn included); its failure typed on `path`. A call given up keeps the
    /// turn until the device answers it.
    async fn call<T, F>(&self, path: &Path, limit: Duration, work: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&dyn Session) -> DeviceResult<T> + Send + 'static,
    {
        let session = Arc::clone(&self.session);
        let turn = Arc::clone(&self.turn);
        let task = async move {
            let held = turn.lock_owned().await;
            tokio::task::spawn_blocking(move || {
                let answer = work(session.as_ref());
                drop(held);
                answer
            })
            .await
        };
        match tokio::time::timeout(limit, task)
            .await
            .map(|joined| joined.log_panic("vfs.device_call"))
        {
            Err(_) => Err(VfsError::TimedOut {
                path: path.to_path_buf(),
                seconds: limit.as_secs(),
            }),
            Ok(Err(error)) => Err(VfsError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::other(error),
            }),
            Ok(Ok(answer)) => answer.map_err(|error| error.on(path)),
        }
    }

    /// The objects of the folder `node` at `at` (the storages at the root), kept in the cache.
    async fn children(&self, at: &Path, node: &Node) -> Result<Vec<Object>> {
        let limit = self.timeouts.listing;
        let children = match node {
            Node::Root => self.call(at, limit, |session| session.storages()).await?,
            Node::Object(object) if object.folder => {
                let id = object.id.clone();
                self.call(at, limit, move |session| session.children(&id))
                    .await?
            }
            Node::Object(_) => {
                return Err(VfsError::NotADirectory {
                    path: at.to_path_buf(),
                });
            }
        };
        self.cache().replace_children(at, &children, Instant::now());
        Ok(children)
    }

    /// What `names` name, and its path as the device spells it.
    async fn resolve(&self, names: &[String], path: &Path) -> Result<(Node, PathBuf)> {
        let mut node = Node::Root;
        let mut at = PathBuf::from("/");
        for name in names {
            let found = self.cache().lookup(&at, name, Instant::now());
            let object = match found {
                Lookup::Found(object) => object,
                Lookup::Absent => {
                    return Err(VfsError::NotFound {
                        path: path.to_path_buf(),
                    });
                }
                Lookup::Unknown => {
                    let children = self.children(&at, &node).await?;
                    find(&children, name).ok_or_else(|| VfsError::NotFound {
                        path: path.to_path_buf(),
                    })?
                }
            };
            at.push(&object.name);
            node = Node::Object(object);
        }
        Ok((node, at))
    }

    /// The folder `names` names, with its path; a file is `not_a_directory`.
    async fn folder(&self, names: &[String], path: &Path) -> Result<(Node, PathBuf)> {
        let (node, at) = self.resolve(names, path).await?;
        if !node.is_folder() {
            return Err(VfsError::NotADirectory {
                path: path.to_path_buf(),
            });
        }
        Ok((node, at))
    }

    /// The object `name` in the folder `node` at `at`, when there is one.
    async fn existing(&self, node: &Node, at: &Path, name: &str) -> Result<Option<Object>> {
        // Looked up apart, so the cache's lock is never held across the listing below.
        let found = self.cache().lookup(at, name, Instant::now());
        match found {
            Lookup::Found(object) => return Ok(Some(object)),
            Lookup::Absent => return Ok(None),
            Lookup::Unknown => {}
        }
        Ok(find(&self.children(at, node).await?, name))
    }

    /// Creates the folder `name` in the folder `parent` at `at`.
    async fn create_folder(
        &self,
        parent: &Node,
        at: &Path,
        name: &str,
        path: &Path,
    ) -> Result<Object> {
        let Node::Object(parent) = parent else {
            return Err(unsupported(path, "creating a storage"));
        };
        let (id, owned) = (parent.id.clone(), name.to_owned());
        let limit = self.timeouts.request;
        let object = self
            .call(path, limit, move |session| {
                session.create_folder(&id, &owned)
            })
            .await?;
        self.cache().put(at.join(&object.name), object.clone());
        Ok(object)
    }

    /// The object at `path`, refused as the device's root or a storage for `operation`.
    async fn below_storage(
        &self,
        path: &Path,
        operation: &'static str,
    ) -> Result<(Object, PathBuf)> {
        let names = names(path)?;
        if names.len() < 2 {
            return Err(unsupported(path, operation));
        }
        match self.resolve(&names, path).await? {
            (Node::Object(object), at) => Ok((object, at)),
            (Node::Root, _) => Err(unsupported(path, operation)),
        }
    }

    /// Deletes `object` at `at`.
    async fn delete(&self, object: &Object, at: &Path, recursive: bool) -> Result<()> {
        let id = object.id.clone();
        let limit = if recursive {
            self.timeouts.listing
        } else {
            self.timeouts.request
        };
        self.call(at, limit, move |session| session.delete(&id, recursive))
            .await?;
        self.cache().forget(at);
        Ok(())
    }
}

#[async_trait]
impl Vfs for DeviceFs {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            rename_across_dirs: false,
            symlinks: false,
            case_sensitive: false,
            unix_mode: false,
            set_mtime: false,
            free_space: true,
        }
    }

    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        let (node, at) = self.resolve(&names(dir)?, dir).await?;
        let children = self
            .children(&at, &node)
            .await
            .map_err(|error| match error {
                VfsError::NotADirectory { .. } => VfsError::NotADirectory {
                    path: dir.to_path_buf(),
                },
                other => other,
            })?;
        Ok(children.iter().map(entry).collect())
    }

    async fn stat(&self, path: &Path) -> Result<Entry> {
        match self.resolve(&names(path)?, path).await? {
            (Node::Root, _) => Ok(entry(&Object {
                id: String::new(),
                name: String::new(),
                folder: true,
                size: 0,
                modified: None,
            })),
            (Node::Object(object), _) => Ok(entry(&object)),
        }
    }

    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        match self.resolve(&names(path)?, path).await? {
            (Node::Object(object), _) if !object.folder => {
                pipe::download(Arc::clone(&self.session), object.id, object.size, path).await
            }
            _ => Err(VfsError::IsADirectory {
                path: path.to_path_buf(),
            }),
        }
    }

    async fn open_write(&self, path: &Path, mode: WriteMode) -> Result<WriteStream> {
        if mode == WriteMode::Append {
            return Err(unsupported(path, "appending to a file on a device"));
        }
        if !self.session.abilities().write {
            return Err(unsupported(path, "writing files to this device"));
        }
        let mut names = names(path)?;
        let name = names.pop().ok_or_else(|| VfsError::IsADirectory {
            path: path.to_path_buf(),
        })?;
        if names.is_empty() {
            return Err(VfsError::InvalidPath {
                path: path.to_path_buf(),
                reason: "a file goes into one of the device's storages",
            });
        }
        let (parent, at) = self.folder(&names, path).await?;
        let replaced = match self.existing(&parent, &at, &name).await? {
            Some(object) if object.folder => {
                return Err(VfsError::IsADirectory {
                    path: path.to_path_buf(),
                });
            }
            Some(_) if mode == WriteMode::CreateNew => {
                return Err(VfsError::AlreadyExists {
                    path: path.to_path_buf(),
                });
            }
            Some(object) => Some(object.id),
            None => None,
        };
        let Node::Object(folder) = parent else {
            return Err(unsupported(path, "files in the device's root"));
        };
        write::spool(Target {
            session: Arc::clone(&self.session),
            cache: Arc::clone(&self.cache),
            folder: folder.id,
            path: at.join(&name),
            name,
            replaced,
        })
    }

    async fn mkdir(&self, path: &Path, recursive: bool) -> Result<()> {
        if !self.session.abilities().mkdir {
            return Err(unsupported(path, "creating folders on this device"));
        }
        let names = names(path)?;
        let Some((name, parents)) = names.split_last() else {
            return if recursive {
                Ok(())
            } else {
                Err(VfsError::AlreadyExists {
                    path: path.to_path_buf(),
                })
            };
        };
        if !recursive {
            let (parent, at) = self.folder(parents, path).await?;
            if self.existing(&parent, &at, name).await?.is_some() {
                return Err(VfsError::AlreadyExists {
                    path: path.to_path_buf(),
                });
            }
            return self.create_folder(&parent, &at, name, path).await.map(drop);
        }
        // Down from the root: what is there is passed, what is missing is created.
        let (mut node, mut at) = (Node::Root, PathBuf::from("/"));
        for name in &names {
            let object = match self.existing(&node, &at, name).await? {
                Some(object) if object.folder => object,
                Some(_) => {
                    return Err(VfsError::NotADirectory {
                        path: path_of(&names),
                    });
                }
                None => self.create_folder(&node, &at, name, path).await?,
            };
            at.push(&object.name);
            node = Node::Object(object);
        }
        Ok(())
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        if !self.session.abilities().rename {
            return Err(unsupported(from, "renaming on this device"));
        }
        let (from_names, to_names) = (names(from)?, names(to)?);
        let (Some((new_name, to_parent)), Some((_, from_parent))) =
            (to_names.split_last(), from_names.split_last())
        else {
            return Err(unsupported(from, "renaming the device"));
        };
        let same_parent = from_parent.len() == to_parent.len()
            && from_parent
                .iter()
                .zip(to_parent)
                .all(|(a, b)| same_name(a, b));
        if !same_parent {
            return Err(unsupported(from, "moving between folders on a device"));
        }
        let (object, at) = self.below_storage(from, "renaming a storage").await?;
        match self.resolve(&to_names, to).await {
            Ok((Node::Object(other), _)) if other.id != object.id => {
                return Err(VfsError::AlreadyExists {
                    path: to.to_path_buf(),
                });
            }
            Ok(_) | Err(VfsError::NotFound { .. }) => {}
            Err(error) => return Err(error),
        }
        let (id, name) = (object.id.clone(), new_name.clone());
        let limit = self.timeouts.request;
        self.call(from, limit, move |session| session.rename(&id, &name))
            .await?;
        let mut cache = self.cache();
        cache.forget(&at);
        if let Some(parent) = at.parent() {
            let renamed = Object {
                name: new_name.clone(),
                ..object
            };
            cache.put(parent.join(new_name), renamed);
        }
        Ok(())
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        if !self.session.abilities().delete {
            return Err(unsupported(path, "deleting on this device"));
        }
        match self.resolve(&names(path)?, path).await? {
            (Node::Object(object), at) if !object.folder => self.delete(&object, &at, false).await,
            _ => Err(VfsError::IsADirectory {
                path: path.to_path_buf(),
            }),
        }
    }

    async fn remove_dir(&self, path: &Path, recursive: bool) -> Result<()> {
        if !self.session.abilities().delete {
            return Err(unsupported(path, "deleting on this device"));
        }
        let (object, at) = self.below_storage(path, "removing a storage").await?;
        if !object.folder {
            return Err(VfsError::NotADirectory {
                path: path.to_path_buf(),
            });
        }
        let node = Node::Object(object.clone());
        if !recursive && !self.children(&at, &node).await?.is_empty() {
            return Err(VfsError::DirectoryNotEmpty {
                path: path.to_path_buf(),
            });
        }
        self.delete(&object, &at, recursive).await
    }

    async fn set_mtime(&self, path: &Path, _modified: SystemTime) -> Result<()> {
        Err(unsupported(path, "setting times on a device"))
    }

    async fn set_attributes(&self, path: &Path, _change: SetAttributes) -> Result<()> {
        Err(unsupported(path, "setting attributes on a device"))
    }

    async fn free_space(&self, path: &Path) -> Result<Option<u64>> {
        let names = names(path)?;
        let Some(storage) = names.first() else {
            return Ok(None);
        };
        match self.resolve(std::slice::from_ref(storage), path).await? {
            (Node::Object(object), _) => {
                let limit = self.timeouts.request;
                self.call(path, limit, move |session| session.free_space(&object.id))
                    .await
            }
            (Node::Root, _) => Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "fs_tests.rs"]
mod tests;
