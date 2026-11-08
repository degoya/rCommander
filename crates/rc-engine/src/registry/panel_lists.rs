//! Total Commander's smaller panel functions (RC-100-92): the file filters (Ctrl+F10, Ctrl+F11,
//! Ctrl+F12) and the saved ones, the ignore list's switch, the selection saved and loaded (in
//! memory, a file, the clipboard's text) and a list file loaded into a panel.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "filters.delete",
        class: CommandClass::Read,
        summary: "Delete a saved file filter by its name",
    },
    CommandSpec {
        id: "filters.list",
        class: CommandClass::Read,
        summary: "The saved file filters (Ctrl+F12), each a name and a mask",
    },
    CommandSpec {
        id: "filters.rename",
        class: CommandClass::Read,
        summary: "Give a saved file filter another name",
    },
    CommandSpec {
        id: "filters.save",
        class: CommandClass::Read,
        summary: "Save a file filter's mask under a name, replacing one of the same name",
    },
    CommandSpec {
        id: "panel.file_filter",
        class: CommandClass::Read,
        summary: "Show all files (Ctrl+F10, also lifts the quick filter), only programs (Ctrl+F11) or the files a mask matches (Ctrl+F12); folders always show",
    },
    CommandSpec {
        id: "panel.load_list",
        class: CommandClass::Read,
        summary: "Show the paths of a list file in a panel as a flat view, missing ones skipped and counted",
    },
    CommandSpec {
        id: "panel.save_selection",
        class: CommandClass::Read,
        summary: "Keep the panel's selection in memory; panel.restore_selection (Num /) brings it back",
    },
    CommandSpec {
        id: "panel.selection_from_file",
        class: CommandClass::Read,
        summary: "Select the entries a text file names, one per line (UTF-8, UTF-16 or ANSI); missing names are counted",
    },
    CommandSpec {
        id: "panel.selection_from_text",
        class: CommandClass::Read,
        summary: "Select the entries a text names, one per line, as the window reads it from the clipboard",
    },
    CommandSpec {
        id: "panel.selection_to_file",
        class: CommandClass::Write,
        summary: "Write the names of the panel's selection to a new text file, one per line (UTF-8)",
    },
    CommandSpec {
        id: "panel.toggle_ignore_list",
        class: CommandClass::Read,
        summary: "Switch the ignore list on or off: the names and paths it lists never show in the panels, the search and copies",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// Ctrl+F10, Ctrl+F11, Ctrl+F12 in Total Commander: what files the panel shows. It
            /// stays when the panel shows another folder and belongs to the tab (or, with
            /// `tabs.filter_per_tab` off, to every tab of the panel). `all` also lifts the quick
            /// filter. From a key without a filter (Ctrl+F12) the window asks.
            "panel.file_filter" => SetFileFilter {
                #[serde(default)]
                panel: Option<PanelId>,
                /// `None` is every file.
                #[serde(default)]
                filter: Option<crate::FileFilter>,
            },
            "filters.list" => ListFileFilters,
            /// A mask under a name: at most 64 characters, the mask at most 1024; one of the same
            /// name (ignoring case) is replaced. At most 50.
            "filters.save" => SaveFileFilter {
                name: String,
                mask: String,
            },
            "filters.rename" => RenameFileFilter {
                name: String,
                new_name: String,
            },
            "filters.delete" => DeleteFileFilter {
                name: String,
            },
            /// `ignore_list.enabled` switched, or set to `enabled`; both panels are read again.
            "panel.toggle_ignore_list" => ToggleIgnoreList {
                #[serde(default)]
                enabled: Option<bool>,
            },
            /// Total Commander's "Save selection": the selection kept in the panel's memory, which
            /// Num / (`panel.restore_selection`) brings back.
            "panel.save_selection" => SaveSelection {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// The selected names into a text file on this machine: UTF-8, one per line, in a
            /// flat view the path below its folder. A file that is there is overwritten only with
            /// `overwrite`, never over MCP.
            "panel.selection_to_file" => SelectionToFile {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Relative to the panel's folder when the panel shows this machine's disks.
                path: String,
                #[serde(default)]
                overwrite: bool,
            },
            /// Selects the entries a text file on this machine names: one per line, a name in
            /// double quotes allowed, at most 16 MiB; a name the panel does not list is counted.
            "panel.selection_from_file" => SelectionFromFile {
                #[serde(default)]
                panel: Option<PanelId>,
                path: String,
            },
            /// As `panel.selection_from_file`, for a text the caller read (the clipboard).
            "panel.selection_from_text" => SelectionFromText {
                #[serde(default)]
                panel: Option<PanelId>,
                text: String,
            },
            /// Total Commander's `LOADLIST`: the absolute paths of a list file on this machine,
            /// one per line, shown as a flat view below their common folder.
            "panel.load_list" => LoadList {
                #[serde(default)]
                panel: Option<PanelId>,
                path: String,
            },
        } }
    };
}
pub(super) use commands;
