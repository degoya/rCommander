//! What a panel keeps of its column view (RC-100-90): how its mode was chosen - by hand or by a
//! column set's folder mask - and the folder's comments while the mode shows them.

use std::{collections::BTreeMap, sync::Arc};

use super::ColumnSetId;
use crate::{ListMode, PanelState, SortKey, comments::Descriptions, sort::names_for};

/// A folder's comments by name, as the comments view and a `comment` column show them. Names
/// compare without regard to case, as `descript.ion` compares them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderComments {
    by_name: BTreeMap<String, String>,
}

impl FolderComments {
    /// The comments of a folder's `descript.ion`.
    pub(crate) fn of(descriptions: &Descriptions) -> Self {
        descriptions.entries().collect()
    }

    /// The comment of the entry `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.by_name.get(&name.to_lowercase()).map(String::as_str)
    }

    /// How many entries have a comment.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Whether no entry has a comment.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

/// The first comment of a name counts, as [`Descriptions::get`] reads it.
impl FromIterator<(String, String)> for FolderComments {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(pairs: T) -> Self {
        let mut by_name = BTreeMap::new();
        for (name, comment) in pairs {
            by_name.entry(name.to_lowercase()).or_insert(comment);
        }
        Self { by_name }
    }
}

/// A panel's column view beyond its [`ListMode`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelColumns {
    /// The mode was chosen by hand (Ctrl+F1, Ctrl+F2, a set from the menu): it stays until the
    /// panel enters a folder for which another set's mask holds (Total Commander).
    pub manual: bool,
    /// The set whose mask matched the folder shown; `None` when none did.
    pub rule: Option<ColumnSetId>,
    /// The mode before the comments view, which Ctrl+Shift+F2 goes back to.
    pub before_comments: Option<ListMode>,
    /// The folder's comments while the mode shows a comment column; `None` until they are read.
    pub comments: Option<Arc<FolderComments>>,
}

impl PanelColumns {
    /// What the panel keeps when it shows a listing: everything, the comments only in the same
    /// folder.
    pub(crate) fn next(&self, same_folder: bool) -> Self {
        Self {
            comments: if same_folder {
                self.comments.clone()
            } else {
                None
            },
            ..self.clone()
        }
    }
}

impl PanelState {
    /// The comment of the entry `name`, when the panel has read the folder's comments.
    #[must_use]
    pub fn comment(&self, name: &str) -> Option<&str> {
        self.columns.comments.as_deref()?.get(name)
    }

    /// With the key [`SortKey::Comment`] and the comments read, the entries sorted by them, the
    /// quick filter and the cursor's entry kept; anything else stays as it is.
    pub(crate) fn order_by_comments(&mut self) {
        if self.sort.key != SortKey::Comment {
            return;
        }
        let Some(comments) = self.columns.comments.clone() else {
            return;
        };
        let names = names_for(&self.path);
        let sorted = crate::sort::sort_by_comments(
            self.listed().to_vec(),
            self.sort.descending,
            names,
            &comments,
        );
        let at = self.cursor_entry().map(|entry| entry.name.clone());
        self.relist(sorted.into());
        self.cursor = at.and_then(|name| self.index_of(&name)).unwrap_or(0);
    }
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
