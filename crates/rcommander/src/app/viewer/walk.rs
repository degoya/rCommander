//! Which files N and P step to in a Lister (RC-100-91), as in Total Commander: F3 with files
//! marked lets them go through the marked files only (`viewer.step_through_selection`, the
//! engine's `ListerFiles::Marked`); Shift+F3 shows the file under the cursor alone and they stay
//! on it (`Single`); else they go through every file of the panel (`Panel`), as before. The list
//! is the window's, kept for each Lister while it is open; the panel's cursor goes along.

use rc_engine::{Command, CursorTarget, Entry, ListerFiles};

use super::{App, Effect, Lister};

/// The files one Lister steps through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Walk {
    /// Every file of the panel.
    Panel,
    /// These names of the panel, the marked ones when it opened.
    Only(Vec<String>),
    /// None: N and P stay on the file.
    Single,
}

impl Walk {
    /// The walk the engine asked for, `marked` the panel's marked names.
    pub fn of(files: ListerFiles, marked: Vec<String>) -> Self {
        match files {
            ListerFiles::Panel => Self::Panel,
            ListerFiles::Marked => Self::Only(marked),
            ListerFiles::Single => Self::Single,
        }
    }

    fn takes(&self, name: &str) -> bool {
        match self {
            Self::Panel => true,
            Self::Only(names) => names.iter().any(|known| known == name),
            Self::Single => false,
        }
    }
}

/// The file after (`forward`) or before `current` among `entries` that `walk` steps to; never a
/// folder.
pub fn next<'a>(
    entries: &'a [Entry],
    current: &str,
    forward: bool,
    walk: &Walk,
) -> Option<&'a Entry> {
    let at = entries.iter().position(|entry| entry.name == current)?;
    let fits = |entry: &&Entry| !entry.is_dir_like() && walk.takes(&entry.name);
    if forward {
        entries[at + 1..].iter().find(fits)
    } else {
        entries[..at].iter().rev().find(fits)
    }
}

impl App {
    /// N and P: the next or previous file of the panel Lister was opened from - of its walk -, in
    /// the same Lister; the panel's cursor goes along, as in Total Commander.
    pub(super) fn step_file(&mut self, id: u64, forward: bool) -> Vec<Effect> {
        let Some(lister) = self.viewers.lister(id) else {
            return Vec::new();
        };
        let panel = lister.panel;
        let pane = self.pane(panel);
        if lister.path.parent().as_ref() != Some(&pane.state.path) {
            return Vec::new();
        }
        let walk = self.viewers.walk(id);
        let Some(step) = next(pane.entries(), &lister.name(), forward, &walk) else {
            return Vec::new();
        };
        let (name, path) = (step.name.clone(), pane.state.path.join(&step.name));
        if let Some(lister) = self.viewers.lister_mut(id) {
            let kept = (lister.wrap, lister.tab_width, lister.text_size);
            *lister = Lister::new(id, panel, path.clone());
            (lister.wrap, lister.tab_width, lister.text_size) = kept;
        }
        let cursor = Command::SetCursor {
            panel: Some(panel),
            target: CursorTarget::Name(name),
        };
        vec![
            Effect::Dispatch(panel, vec![cursor]),
            self.open_file(id, path),
        ]
    }
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
