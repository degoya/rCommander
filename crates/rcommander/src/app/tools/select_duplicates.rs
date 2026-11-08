//! "Select duplicate files" (RC-100-98), Total Commander's dialog behind Num+ when the panel shows
//! the result of a duplicate search: the tab "Files" selects by mask (or removes by mask), by date
//! or by size within each group; the tab "Select by folder" lists the result's folders, marks them
//! one by one, by a pattern, all, none or the other way round, and selects the files in the marked
//! ones (with their subfolders if asked). "Leave at least one file in each group unselected" with
//! its three criteria holds for both. OK applies and closes, Apply applies and stays; the engine's
//! `panel.select_duplicates` changes only the selection, F8 then deletes the copies. The choice
//! stays for the session.

use std::collections::BTreeSet;

use rc_engine::{
    Chord, Command, FlatSource, PanelId,
    search::{
        DatePick, DuplicateRule, DuplicateSelection, Keep, SizePick, default_keep,
        duplicate_folders, folder_matches,
    },
};

use super::ToolsMsg;
use crate::app::{App, Effect, Message, dialog_windows::DialogWindow, field_walk};

/// The dialog's two tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelTab {
    Files,
    Folders,
}

/// The rules of the tab "Files", one chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesRule {
    SelectMask,
    RemoveMask,
    Date(DatePick),
    Size(SizePick),
}

impl FilesRule {
    pub const ALL: [Self; 10] = [
        Self::SelectMask,
        Self::RemoveMask,
        Self::Date(DatePick::AllOlder),
        Self::Date(DatePick::AllNewer),
        Self::Date(DatePick::Oldest),
        Self::Date(DatePick::Newest),
        Self::Size(SizePick::AllLarger),
        Self::Size(SizePick::AllSmaller),
        Self::Size(SizePick::Largest),
        Self::Size(SizePick::Smallest),
    ];
}

/// The dialog, open or closed with its choice kept.
#[derive(Debug)]
pub struct SelectDuplicates {
    pub open: bool,
    /// The panel whose duplicate result it selects in.
    pub panel: PanelId,
    pub tab: SelTab,
    pub rule: FilesRule,
    pub mask: String,
    /// The folders of the result, as the panel names them (empty: the panel's folder).
    pub folders: Vec<String>,
    /// The folders marked in the list.
    pub marked: BTreeSet<String>,
    pub pattern: String,
    pub subfolders: bool,
    pub parent_only: bool,
    pub leave_one: bool,
    /// The three criteria that pick the file left in a group.
    pub keep: [Keep; 3],
}

impl SelectDuplicates {
    fn new(panel: PanelId) -> Self {
        let keep = default_keep();
        Self {
            open: true,
            panel,
            tab: SelTab::Files,
            rule: FilesRule::SelectMask,
            mask: "*.*".to_owned(),
            folders: Vec::new(),
            marked: BTreeSet::new(),
            pattern: String::new(),
            subfolders: false,
            parent_only: false,
            leave_one: true,
            keep: [
                keep.first().copied().unwrap_or(Keep::Newest),
                keep.get(1).copied().unwrap_or(Keep::Largest),
                keep.get(2).copied().unwrap_or(Keep::First),
            ],
        }
    }

    /// What OK and Apply send to the engine.
    pub fn spec(&self) -> DuplicateSelection {
        let rule = match (self.tab, self.rule) {
            (SelTab::Folders, _) => DuplicateRule::Folders {
                folders: self.marked.iter().cloned().collect(),
                pattern: None,
                parent_only: self.parent_only,
                subfolders: self.subfolders,
                remove: false,
            },
            (SelTab::Files, FilesRule::SelectMask) => DuplicateRule::Mask {
                mask: self.mask.clone(),
                remove: false,
            },
            (SelTab::Files, FilesRule::RemoveMask) => DuplicateRule::Mask {
                mask: self.mask.clone(),
                remove: true,
            },
            (SelTab::Files, FilesRule::Date(pick)) => DuplicateRule::Date { pick },
            (SelTab::Files, FilesRule::Size(pick)) => DuplicateRule::Size { pick },
        };
        DuplicateSelection {
            rule,
            leave_one: self.leave_one,
            keep: self.keep.to_vec(),
        }
    }

    /// The pattern marks (or, with `mark` off, unmarks) the folders it matches.
    fn by_pattern(&mut self, mark: bool) {
        let pattern = self.pattern.trim();
        if pattern.is_empty() {
            return;
        }
        for folder in &self.folders {
            if folder_matches(folder, pattern, self.parent_only) {
                if mark {
                    self.marked.insert(folder.clone());
                } else {
                    self.marked.remove(folder);
                }
            }
        }
    }
}

/// What the dialog's controls say.
#[derive(Debug, Clone)]
pub enum SelMsg {
    Tab(SelTab),
    Rule(FilesRule),
    Mask(String),
    /// A folder of the list marked or unmarked.
    Folder(String, bool),
    Pattern(String),
    /// "Select" and "Remove" beside the pattern.
    PatternMark(bool),
    All,
    Nothing,
    Reverse,
    Subfolders(bool),
    ParentOnly(bool),
    LeaveOne(bool),
    /// Criterion 1, 2 or 3 of "leave one".
    Keep(usize, Keep),
    Ok,
    Apply,
    Cancel,
}

pub(super) fn wrap(message: SelMsg) -> Message {
    Message::Tools(ToolsMsg::Select(message))
}

impl App {
    /// Num+ over a duplicate result: the dialog opens for `panel`, with the last choice.
    pub(in crate::app) fn open_select_duplicates(&mut self, panel: PanelId) -> Vec<Effect> {
        let names = self.pane(panel).state.entries.clone();
        let folders = duplicate_folders(names.iter().map(|entry| entry.name.as_str()));
        let dialog = self
            .tools
            .select
            .get_or_insert_with(|| SelectDuplicates::new(panel));
        dialog.open = true;
        dialog.panel = panel;
        dialog.marked.retain(|folder| folders.contains(folder));
        dialog.folders = folders;
        self.dialog_window_front(DialogWindow::SelectDuplicates)
    }

    /// Whether Num+ in `panel` opens the dialog: the panel shows a duplicate result.
    pub(in crate::app) fn shows_duplicates(&self, panel: PanelId) -> bool {
        self.pane(panel)
            .state
            .flat
            .as_ref()
            .is_some_and(|flat| flat.source == FlatSource::Duplicates)
    }

    pub(super) fn on_select_duplicates(&mut self, message: SelMsg) -> Vec<Effect> {
        let Some(dialog) = self.tools.select.as_mut().filter(|dialog| dialog.open) else {
            return Vec::new();
        };
        match message {
            SelMsg::Tab(tab) => dialog.tab = tab,
            SelMsg::Rule(rule) => dialog.rule = rule,
            SelMsg::Mask(mask) => dialog.mask = mask,
            SelMsg::Folder(folder, on) => {
                if on {
                    dialog.marked.insert(folder);
                } else {
                    dialog.marked.remove(&folder);
                }
            }
            SelMsg::Pattern(pattern) => dialog.pattern = pattern,
            SelMsg::PatternMark(mark) => dialog.by_pattern(mark),
            SelMsg::All => dialog.marked = dialog.folders.iter().cloned().collect(),
            SelMsg::Nothing => dialog.marked.clear(),
            SelMsg::Reverse => {
                dialog.marked = dialog
                    .folders
                    .iter()
                    .filter(|folder| !dialog.marked.contains(*folder))
                    .cloned()
                    .collect();
            }
            SelMsg::Subfolders(on) => dialog.subfolders = on,
            SelMsg::ParentOnly(on) => dialog.parent_only = on,
            SelMsg::LeaveOne(on) => dialog.leave_one = on,
            SelMsg::Keep(at, keep) => {
                if let Some(slot) = dialog.keep.get_mut(at) {
                    *slot = keep;
                }
            }
            SelMsg::Ok => return self.apply_duplicate_selection(true),
            SelMsg::Apply => return self.apply_duplicate_selection(false),
            SelMsg::Cancel => dialog.open = false,
        }
        Vec::new()
    }

    /// OK (`close`) or Apply: the selection changes in the panel.
    fn apply_duplicate_selection(&mut self, close: bool) -> Vec<Effect> {
        let Some(dialog) = self.tools.select.as_mut().filter(|dialog| dialog.open) else {
            return Vec::new();
        };
        let panel = dialog.panel;
        let select = Command::SelectDuplicates {
            panel: Some(panel),
            spec: dialog.spec(),
        };
        if close {
            dialog.open = false;
        }
        vec![Effect::Dispatch(panel, vec![select])]
    }

    /// A key while the dialog is open: Tab walks its ring, Enter is OK, Escape Cancel.
    pub(in crate::app) fn select_duplicates_key(&mut self, chord: &Chord) -> Vec<Effect> {
        if let Some(effects) = field_walk::walk_key(chord) {
            return effects;
        }
        if !chord.is_plain() {
            return Vec::new();
        }
        match chord.key.as_str() {
            "Escape" => self.on_select_duplicates(SelMsg::Cancel),
            "Enter" => self.on_select_duplicates(SelMsg::Ok),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "select_duplicates_tests.rs"]
mod tests;
