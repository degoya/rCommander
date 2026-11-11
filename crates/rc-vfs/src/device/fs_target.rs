//! Where a file written onto a device goes: its folder and the file it replaces, found and
//! checked before the first byte, for the spooled and the streamed write alike.

use std::{path::Path, sync::Arc};

use super::{DeviceFs, Node};
use crate::device::{
    names::{names, unsupported},
    write::Target,
};
use crate::{Result, VfsError, WriteMode};

impl DeviceFs {
    /// The target of a file written at `path` in `mode`; refused here when MTP or the device
    /// cannot write it, or `mode` does not allow it.
    pub(super) async fn target(&self, path: &Path, mode: WriteMode) -> Result<Target> {
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
        Ok(Target {
            session: Arc::clone(&self.session),
            cache: Arc::clone(&self.cache),
            folder: folder.id,
            path: at.join(&name),
            name,
            replaced,
        })
    }
}
