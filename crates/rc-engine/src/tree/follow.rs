//! When a panel follows a tree's cursor (RC-100-88): the other panel of a tree in a panel, the
//! own panel of a separate tree. It shows the folder under the cursor once the cursor rested for
//! [`FOLLOW_PAUSE`], so arrowing through `C:\Windows` reads only the folder stopped at; with
//! `tree.open_on_enter` on (Total Commander's "Tree: Press ENTER to open Dir in other window")
//! only Enter shows it. The window's timer reports each pause by the turn of the move that
//! started it; a later move makes an earlier pause count for nothing.

use std::time::Duration;

use rc_vfs::VfsPath;

/// The rest after the last move of the cursor before the panel follows it.
pub const FOLLOW_PAUSE: Duration = Duration::from_millis(300);

/// One tree's following panel: what waits to be shown and what was shown last.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Follow {
    /// Only Enter shows the folder under the cursor.
    on_enter: bool,
    /// The moves of the cursor so far.
    turn: u64,
    /// The folder under the cursor not shown yet, and the turn that put the cursor there.
    pending: Option<(VfsPath, u64)>,
    /// The folder the panel shows, as far as the tree knows.
    shown: Option<VfsPath>,
}

impl Follow {
    /// A panel that follows after the pause, or with `on_enter` only after Enter.
    #[must_use]
    pub fn new(on_enter: bool) -> Self {
        Self {
            on_enter,
            ..Self::default()
        }
    }

    /// `tree.open_on_enter` changed.
    pub fn set_on_enter(&mut self, on_enter: bool) {
        self.on_enter = on_enter;
    }

    /// The cursor moved to `path`: the turn whose pause to wait for, or `None` when the panel
    /// shows it already or only Enter shows it.
    pub fn moved(&mut self, path: VfsPath) -> Option<u64> {
        self.turn += 1;
        if self.shown.as_ref() == Some(&path) {
            self.pending = None;
            return None;
        }
        self.pending = Some((path, self.turn));
        (!self.on_enter).then_some(self.turn)
    }

    /// The pause after the move of `turn` ended: the folder to show, when no later move came.
    pub fn rested(&mut self, turn: u64) -> Option<VfsPath> {
        if self.on_enter {
            return None;
        }
        match &self.pending {
            Some((_, waited)) if *waited == turn => self.take(),
            _ => None,
        }
    }

    /// Enter on `cursor`: the folder to show now, whatever waited.
    pub fn enter(&mut self, cursor: VfsPath) -> VfsPath {
        self.pending = None;
        self.shown = Some(cursor.clone());
        cursor
    }

    /// The panel shows `path` now - it moved by itself, or followed: nothing waits for a folder
    /// it shows.
    pub fn showing(&mut self, path: &VfsPath) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(waiting, _)| waiting == path)
        {
            self.pending = None;
        }
        self.shown = Some(path.clone());
    }

    /// Nothing waits any more: the tree's cursor went with its panel, which moved by itself.
    pub fn forget(&mut self) {
        self.pending = None;
    }

    /// Whether a folder waits to be shown: the tree does not follow its panel meanwhile.
    #[must_use]
    pub fn waiting(&self) -> bool {
        self.pending.is_some()
    }

    fn take(&mut self) -> Option<VfsPath> {
        let (path, _) = self.pending.take()?;
        self.shown = Some(path.clone());
        Some(path)
    }
}

#[cfg(test)]
#[path = "follow_tests.rs"]
mod tests;
