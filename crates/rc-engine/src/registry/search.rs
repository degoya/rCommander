//! The file search of the find-files dialog and the flat view of a panel it shares with
//! Total Commander's branch view (RC-020-03).

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::{PanelId, SearchQuery};

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

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "files.find_dialog",
        class: CommandClass::Read,
        summary: "Open the find-files dialog in the window, starting in a panel's folder",
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
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "files.search" => SearchFiles(SearchRequest),
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
