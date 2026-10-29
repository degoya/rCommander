//! Files on the clipboard as in Total Commander and the Explorer (RC-080-01): Ctrl+C copies the
//! active panel's selection (or the entry under the cursor) to the clipboard, Ctrl+X cuts it,
//! Ctrl+V copies (after a cut: moves) the files on the clipboard into the active panel's folder
//! through the operations queue. Remote entries are fetched into the temporary folder first and
//! can only be copied. The keys are the window's (`keymap/default.json` reserves them): the
//! system's clipboard is no engine state.

use std::path::PathBuf;

use rc_engine::{Chord, PanelId, PanelView};

use super::{
    DragMsg,
    fetch::{self, Purpose},
    wrap,
};
use crate::{
    app::{App, Effect},
    i18n::t,
    platform::clipboard_files::{self, ClipboardFiles},
};

impl App {
    /// Ctrl+C, Ctrl+X and Ctrl+V in a file list; `None` for any other key.
    pub(in crate::app) fn clipboard_key(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        if !chord.ctrl || chord.shift || chord.alt || !clipboard_files::AVAILABLE {
            return None;
        }
        let panel = self.active;
        if self.pane(panel).state.view != PanelView::Files {
            return None;
        }
        match chord.key.as_str() {
            "c" => Some(self.copy_files(panel, false)),
            "x" => Some(self.copy_files(panel, true)),
            "v" => Some(paste()),
            _ => None,
        }
    }

    /// Ctrl+C (`cut` false) or Ctrl+X: the entries of `panel` onto the clipboard.
    fn copy_files(&mut self, panel: PanelId, cut: bool) -> Vec<Effect> {
        let pane = self.pane(panel);
        let names = pane.chosen_names();
        if names.is_empty() {
            return Vec::new();
        }
        let folder = pane.state.path.clone();
        if !fetch::remote(&folder) {
            let paths = names.iter().map(|name| folder.path.join(name)).collect();
            return self.put_on_clipboard(paths, cut);
        }
        if cut {
            self.pane_mut(panel).error = Some(t!("clipboard.cut_remote").to_owned());
            return Vec::new();
        }
        self.fetch(panel, names, Purpose::Clipboard)
    }

    /// Puts `paths` on the clipboard, to be copied (or, `cut`, moved) where they are pasted.
    pub(super) fn put_on_clipboard(&mut self, paths: Vec<PathBuf>, cut: bool) -> Vec<Effect> {
        let count = paths.len();
        clipboard_files::write(ClipboardFiles { paths, cut })
            .map(|task| Effect::Task(task.map(move |ok| wrap(DragMsg::Copied { count, cut, ok }))))
            .into_iter()
            .collect()
    }

    /// The clipboard took the files, or did not.
    pub(super) fn copied(&mut self, count: usize, cut: bool, ok: bool) {
        if !ok {
            let active = self.active;
            self.pane_mut(active).error = Some(t!("clipboard.failed").to_owned());
        } else if cut {
            self.info = Some(t!("clipboard.cut", count = count));
        } else {
            self.info = Some(t!("clipboard.copied", count = count));
        }
    }

    /// What Ctrl+V found: files go into the active panel's folder, as a drop there would.
    pub(super) fn pasted(&mut self, files: Option<ClipboardFiles>) -> Vec<Effect> {
        let Some(files) = files.filter(|files| !files.paths.is_empty()) else {
            self.info = Some(t!("clipboard.no_files").to_owned());
            return Vec::new();
        };
        let active = self.active;
        self.copy_in(active, files.paths, files.cut)
    }
}

/// Ctrl+V: reads the clipboard; [`DragMsg::Pasted`] brings what it holds.
fn paste() -> Vec<Effect> {
    clipboard_files::read()
        .map(|task| Effect::Task(task.map(|files| wrap(DragMsg::Pasted(files)))))
        .into_iter()
        .collect()
}
