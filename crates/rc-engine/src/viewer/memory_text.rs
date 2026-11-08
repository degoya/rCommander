//! A text the window made, shown by the Lister as if it were a file, without touching a disk
//! (RC-110-08): the certificate report. [`MemoryText`] is a read-only backend of exactly one
//! file whose bytes are held in memory; [`open_text`] hands the Lister a [`FileWindow`] over it,
//! so paging, search, selection and the encodings work as on any file. Every write is refused.

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use async_trait::async_trait;
use rc_vfs::{
    Attributes, Capabilities, Entry, EntryKind, ReadStream, Result, SetAttributes, Vfs, VfsError,
    WriteMode, WriteStream,
};

use super::FileWindow;

/// One file in memory: its path, its bytes and when it was made.
#[derive(Debug)]
pub struct MemoryText {
    path: PathBuf,
    bytes: Arc<[u8]>,
    made: SystemTime,
}

/// `text` as a file named `name` for the Lister; nothing is written anywhere.
///
/// # Errors
///
/// None in practice: the one file is always there.
pub async fn open_text(name: &str, text: String) -> Result<FileWindow> {
    let path = PathBuf::from(name);
    let backend = MemoryText {
        path: path.clone(),
        bytes: Arc::from(text.into_bytes()),
        made: SystemTime::now(),
    };
    FileWindow::open(Arc::new(backend), path).await
}

impl MemoryText {
    fn check(&self, path: &Path) -> Result<()> {
        if path == self.path {
            Ok(())
        } else {
            Err(VfsError::NotFound {
                path: path.to_path_buf(),
            })
        }
    }

    fn stream_from(&self, offset: u64) -> ReadStream {
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.bytes.len());
        let rest = self.bytes.get(start..).unwrap_or_default().to_vec();
        Box::new(io::Cursor::new(rest))
    }
}

/// Every change is refused: the text is the window's, not a file.
fn read_only(path: &Path) -> VfsError {
    VfsError::Unsupported {
        path: path.to_path_buf(),
        operation: "changing a text shown from memory",
    }
}

#[async_trait]
impl Vfs for MemoryText {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            rename_across_dirs: false,
            symlinks: false,
            case_sensitive: true,
            unix_mode: false,
            set_mtime: false,
            free_space: false,
        }
    }

    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        Err(VfsError::NotFound {
            path: dir.to_path_buf(),
        })
    }

    async fn stat(&self, path: &Path) -> Result<Entry> {
        self.check(path)?;
        Ok(Entry {
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            kind: EntryKind::File,
            size: self.bytes.len() as u64,
            modified: Some(self.made),
            created: None,
            accessed: None,
            attributes: Attributes {
                mode: None,
                readonly: true,
                windows: None,
            },
            hidden: false,
            owner: None,
        })
    }

    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        self.check(path)?;
        Ok(self.stream_from(0))
    }

    async fn open_read_at(&self, path: &Path, offset: u64) -> Result<ReadStream> {
        self.check(path)?;
        Ok(self.stream_from(offset))
    }

    async fn open_write(&self, path: &Path, _mode: WriteMode) -> Result<WriteStream> {
        Err(read_only(path))
    }

    async fn mkdir(&self, path: &Path, _recursive: bool) -> Result<()> {
        Err(read_only(path))
    }

    async fn rename(&self, from: &Path, _to: &Path) -> Result<()> {
        Err(read_only(from))
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        Err(read_only(path))
    }

    async fn remove_dir(&self, path: &Path, _recursive: bool) -> Result<()> {
        Err(read_only(path))
    }

    async fn set_mtime(&self, path: &Path, _modified: SystemTime) -> Result<()> {
        Err(read_only(path))
    }

    async fn set_attributes(&self, path: &Path, _change: SetAttributes) -> Result<()> {
        Err(read_only(path))
    }
}

#[cfg(test)]
#[path = "memory_text_tests.rs"]
mod tests;
