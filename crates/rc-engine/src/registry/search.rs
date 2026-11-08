//! The file search of the find-files dialog and the flat view of a panel it shares with
//! Total Commander's branch view (RC-020-03).

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::{PanelId, SearchQuery, search::DuplicateQuery};

/// The parameters of `files.search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    /// The panel whose folder relative paths start from; the active one by default.
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// The start folders; the panel's folder when empty.
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub query: SearchQuery,
    /// Hits per page: [`crate::DEFAULT_PAGE`] by default, at most [`crate::MAX_PAGE`].
    #[serde(default)]
    pub limit: Option<usize>,
    /// The `next_cursor` of the page before; the first page when `None`.
    #[serde(default)]
    pub cursor: Option<String>,
}

/// The parameters of `files.find_duplicates` (RC-100-98): those of `files.search`, and what makes
/// two files duplicates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicatesRequest {
    /// The panel whose folder relative paths start from; the active one by default.
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// The start folders; the panel's folder when empty.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Which files are compared; its own `duplicates` is replaced by the one below.
    #[serde(default)]
    pub query: SearchQuery,
    pub duplicates: DuplicateQuery,
    /// Groups per page: [`crate::DEFAULT_PAGE`] by default, at most [`crate::MAX_PAGE`].
    #[serde(default)]
    pub limit: Option<usize>,
    /// The `next_cursor` of the page before; the first page when `None`.
    #[serde(default)]
    pub cursor: Option<String>,
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "files.find_dialog",
        class: CommandClass::Read,
        summary: "Open the find-files dialog in the window, starting in a panel's folder",
    },
    CommandSpec {
        id: "files.find_duplicates",
        class: CommandClass::Read,
        summary: "Find duplicate files below start folders by size, name and contents, group by group",
    },
    CommandSpec {
        id: "files.search",
        class: CommandClass::Read,
        summary: "Find files and folders by name masks or regex, content, size, date and attributes, page by page",
    },
    CommandSpec {
        id: "panel.branch_view",
        class: CommandClass::Read,
        summary: "Show every file below a panel's folder (or its selected folders) flat, or leave that view",
    },
    CommandSpec {
        id: "panel.feed_paths",
        class: CommandClass::Read,
        summary: "Show given files and folders, such as search hits, flat in a panel",
    },
    CommandSpec {
        id: "panel.select_duplicates",
        class: CommandClass::Read,
        summary: "Select files of a duplicate search result in a panel by mask, date, size or folder, leaving one per group if asked",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "files.search" => SearchFiles(SearchRequest),
            /// Total Commander's "Find duplicate files": the groups of a duplicate search, page by
            /// page (RC-100-98).
            "files.find_duplicates" => FindDuplicates(DuplicatesRequest),
            /// Ctrl+B in Total Commander: every file below the panel's folder in one flat list,
            /// named by its relative path; again, it leaves into the folder of the entry under the
            /// cursor.
            "panel.branch_view" => BranchView {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Ctrl+Shift+B: only the selected folders and files (or the entry under the
                /// cursor).
                #[serde(default)]
                selection: bool,
                /// Start (`true`) or leave (`false`) the flat view; `None` switches.
                #[serde(default)]
                show: Option<bool>,
            },
            /// "Feed to listbox": the panel shows these entries flat, as Ctrl+B shows a branch.
            "panel.feed_paths" => FeedPaths {
                #[serde(default)]
                panel: Option<PanelId>,
                /// The folder the panel shows, absolute or relative to its current one; names are
                /// relative to it. The panel's folder when `None`.
                #[serde(default)]
                root: Option<String>,
                /// Files and folders, absolute or relative to `root`; at most `MAX_FLAT_ENTRIES`.
                paths: Vec<String>,
                /// The duplicate group of each path, in the order of `paths` (RC-100-98): the
                /// panel then shows a duplicate result, which `panel.select_duplicates` works on.
                /// Empty for plain paths.
                #[serde(default, skip_serializing_if = "Vec::is_empty")]
                groups: Vec<u32>,
            },
            /// Num+ over a duplicate result: Total Commander's "Select duplicate files"
            /// (RC-100-98).
            "panel.select_duplicates" => SelectDuplicates {
                #[serde(default)]
                panel: Option<PanelId>,
                spec: DuplicateSelection,
            },
            /// Alt+F7: the window opens the find-files dialog for the panel.
            "files.find_dialog" => FindFilesDialog {
                #[serde(default)]
                panel: Option<PanelId>,
            },
        } }
    };
}
pub(super) use commands;
