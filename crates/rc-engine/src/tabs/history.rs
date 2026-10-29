//! A tab's folder history: the folders it showed, oldest first, and where in them it stands.
//!
//! As a browser's: a new folder drops what lay ahead (Alt+Right after Alt+Left), going back and
//! forward only moves the place. At most [`HISTORY_LIMIT`] folders are kept; the oldest go first.

use rc_vfs::VfsPath;

/// The folders one tab keeps (Total Commander keeps a similar number).
pub const HISTORY_LIMIT: usize = 50;

/// The folders a tab showed and the one it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    /// Oldest first; never empty.
    entries: Vec<VfsPath>,
    /// Index of the folder shown.
    pos: usize,
}

impl History {
    /// The history of a tab that has shown only `path`.
    #[must_use]
    pub fn new(path: VfsPath) -> Self {
        Self {
            entries: vec![path],
            pos: 0,
        }
    }

    /// A history read back: `back` oldest first, the folder shown, `ahead` nearest first; cut to
    /// [`HISTORY_LIMIT`], the farthest folders first.
    #[must_use]
    pub fn restore(back: Vec<VfsPath>, current: VfsPath, ahead: Vec<VfsPath>) -> Self {
        let keep_back = back.len().min(HISTORY_LIMIT - 1);
        let keep_ahead = ahead.len().min(HISTORY_LIMIT - 1 - keep_back);
        let mut entries: Vec<VfsPath> = back.into_iter().rev().take(keep_back).collect();
        entries.reverse();
        let pos = entries.len();
        entries.push(current);
        entries.extend(ahead.into_iter().take(keep_ahead));
        Self { entries, pos }
    }

    /// The folder shown.
    #[must_use]
    pub fn current(&self) -> &VfsPath {
        &self.entries[self.pos]
    }

    /// Every folder, oldest first.
    #[must_use]
    pub fn entries(&self) -> &[VfsPath] {
        &self.entries
    }

    /// Index of the folder shown in [`History::entries`].
    #[must_use]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// The tab now shows `path`. The same folder again changes nothing; another one drops the
    /// folders ahead and becomes the newest.
    pub fn visit(&mut self, path: &VfsPath) {
        if self.current() == path {
            return;
        }
        self.entries.truncate(self.pos + 1);
        self.entries.push(path.clone());
        if self.entries.len() > HISTORY_LIMIT {
            let excess = self.entries.len() - HISTORY_LIMIT;
            self.entries.drain(..excess);
        }
        self.pos = self.entries.len() - 1;
    }

    /// The folder one step back (`-1`) or ahead (`1`), or any number of steps.
    #[must_use]
    pub fn step(&self, by: isize) -> Option<(usize, &VfsPath)> {
        let index = self.pos.checked_add_signed(by)?;
        self.entries.get(index).map(|path| (index, path))
    }

    /// Stands on entry `index`; `false` when there is none.
    pub fn go(&mut self, index: usize) -> bool {
        if index < self.entries.len() {
            self.pos = index;
            true
        } else {
            false
        }
    }

    /// Every folder but the one shown, with its distance from it: negative behind, positive
    /// ahead.
    pub fn steps(&self) -> impl Iterator<Item = (i64, &VfsPath)> + '_ {
        let pos = i64::try_from(self.pos).unwrap_or(i64::MAX);
        self.entries
            .iter()
            .enumerate()
            .filter(move |(index, _)| *index != self.pos)
            .map(move |(index, path)| (i64::try_from(index).unwrap_or(i64::MAX) - pos, path))
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
