//! Quick View (Ctrl+Q): the other panel shows the entry under this panel's cursor and follows
//! the cursor - a file as the viewer shows it, a directory as its counts and the size of its
//! files (the size of its subdirectories is not known until it is calculated). Ctrl+Q again
//! gives the panel its own view back.
//!
//! The panel the cursor moves in keeps the keys; Tab into the Quick View gives them to the
//! viewer there (scrolling, modes, search, text size), and Tab back returns them. A text size
//! set there stays while the cursor moves on (RC-020-12).

use rc_engine::{Entry, PanelId, VfsPath};

use super::lister::Lister;

/// The Quick View of one panel's cursor.
#[derive(Debug)]
pub struct Quick {
    /// The panel whose cursor it follows; the other one shows it.
    pub source: PanelId,
    pub shown: QuickShown,
    /// The text size the files are shown at while the cursor moves on; `None` for the
    /// settings' (RC-020-12).
    pub text_size: Option<f32>,
}

impl Quick {
    pub fn new(source: PanelId) -> Self {
        Self {
            source,
            shown: QuickShown::Nothing,
            text_size: None,
        }
    }

    /// `lister` shown, at the text size of the Quick View.
    pub fn show_file(&mut self, mut lister: Lister) {
        lister.text_size = self.text_size;
        self.shown = QuickShown::File(Box::new(lister));
    }

    /// Whether the file shown is the lister `id`'s.
    pub fn shows(&self, id: u64) -> bool {
        matches!(&self.shown, QuickShown::File(lister) if lister.id == id)
    }

    /// The panel that shows the Quick View.
    pub fn target(&self) -> PanelId {
        self.source.other()
    }

    /// The path shown now.
    pub fn path(&self) -> Option<&VfsPath> {
        match &self.shown {
            QuickShown::File(lister) => Some(&lister.path),
            QuickShown::Dir { path, .. } => Some(path),
            QuickShown::Nothing => None,
        }
    }
}

/// What the Quick View shows.
#[derive(Debug)]
pub enum QuickShown {
    /// The cursor is on `..` or the directory is empty.
    Nothing,
    File(Box<Lister>),
    Dir {
        path: VfsPath,
        /// `None` while it is listed.
        summary: Option<Result<DirSummary, String>>,
    },
}

/// A directory's entries counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirSummary {
    pub files: u64,
    pub dirs: u64,
    /// The bytes of its files, not of its subdirectories.
    pub bytes: u64,
}

impl DirSummary {
    pub fn of(entries: &[Entry]) -> Self {
        entries.iter().fold(Self::default(), |mut summary, entry| {
            if entry.is_dir_like() {
                summary.dirs += 1;
            } else {
                summary.files += 1;
                summary.bytes += entry.size;
            }
            summary
        })
    }
}

/// What the Quick View is to show for the entry under the cursor of a panel showing `dir`:
/// `None` on `..` or in an empty directory.
pub fn wanted(dir: &VfsPath, entry: Option<&Entry>) -> Option<(VfsPath, bool)> {
    entry.map(|entry| (dir.join(&entry.name), entry.is_dir_like()))
}

#[cfg(test)]
#[path = "quick_tests.rs"]
mod tests;
