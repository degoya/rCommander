//! The two panels: folder, cursor, selection, view (RC-010-01).

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::PanelId;

/// The parameters of `panel.select_by_mask` and `panel.deselect_by_mask`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskRequest {
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// `None` is `*.*`; from a key the window asks for the mask first.
    #[serde(default)]
    pub mask: Option<String>,
}

/// The step of `panel.thumbnail_zoom` (RC-100-89).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoomStep {
    /// The next larger tile.
    In,
    /// The next smaller tile.
    Out,
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "panel.cancel_io",
        class: CommandClass::Read,
        summary: "Stop the calls that wait on a drive or share that does not answer; they end with cancelled",
    },
    CommandSpec {
        id: "panel.change_drive",
        class: CommandClass::Read,
        summary: "Show a drive in a panel, in the directory the panel last showed on it",
    },
    CommandSpec {
        id: "panel.context_menu",
        class: CommandClass::Read,
        summary: "Open the context menu of a panel's selection in the window",
    },
    CommandSpec {
        id: "panel.cycle_separate_tree",
        class: CommandClass::Read,
        summary: "Turn the folder tree beside the panels off, to one for both, to one beside each, and off again (Ctrl+Shift+F8)",
    },
    CommandSpec {
        id: "panel.deselect_all",
        class: CommandClass::Read,
        summary: "Deselect every entry of a panel",
    },
    CommandSpec {
        id: "panel.deselect_by_mask",
        class: CommandClass::Read,
        summary: "Deselect the files matching a Total Commander mask such as *.txt;*.md (directories with a trailing backslash)",
    },
    CommandSpec {
        id: "panel.drive_menu",
        class: CommandClass::Read,
        summary: "Open a panel's drive menu in the window",
    },
    CommandSpec {
        id: "panel.get_state",
        class: CommandClass::Read,
        summary: "A panel's path, cursor, selection and entry count; which panel is active",
    },
    CommandSpec {
        id: "panel.git_status",
        class: CommandClass::Read,
        summary: "The git marks and branch a panel knows of its folder; starts no process",
    },
    CommandSpec {
        id: "panel.invert_selection",
        class: CommandClass::Read,
        summary: "Select the entries of a panel that are not selected, and deselect the others",
    },
    CommandSpec {
        id: "panel.list_dir",
        class: CommandClass::Read,
        summary: "The entries of a directory (a panel's by default), without moving a panel",
    },
    CommandSpec {
        id: "panel.move_cursor",
        class: CommandClass::Read,
        summary: "Move a panel's cursor a step: up, down, a page, to the first or last entry",
    },
    CommandSpec {
        id: "panel.navigate",
        class: CommandClass::Read,
        summary: "Show another directory in a panel (absolute, relative or ..)",
    },
    CommandSpec {
        id: "panel.open_entry",
        class: CommandClass::Read,
        summary: "Open the entry under a panel's cursor: a directory is shown in the panel",
    },
    CommandSpec {
        id: "panel.open_terminal_here",
        class: CommandClass::Exec,
        summary: "Start a shell of the default profile in a panel's folder, shown by that panel or the other one",
    },
    CommandSpec {
        id: "panel.refresh",
        class: CommandClass::Read,
        summary: "Read a panel's directory again, keeping cursor and selection where they can stay; a flat view (branch view, find results) returns to its folder",
    },
    CommandSpec {
        id: "panel.restore_selection",
        class: CommandClass::Read,
        summary: "Select again what was selected before the last file operation, as far as it is still there",
    },
    CommandSpec {
        id: "panel.select_all",
        class: CommandClass::Read,
        summary: "Select every entry of a panel, directories included",
    },
    CommandSpec {
        id: "panel.select_by_mask",
        class: CommandClass::Read,
        summary: "Select the files matching a Total Commander mask such as *.txt;*.md (directories with a trailing backslash)",
    },
    CommandSpec {
        id: "panel.select_extension",
        class: CommandClass::Read,
        summary: "Select or deselect every file with the extension of the entry under the cursor",
    },
    CommandSpec {
        id: "panel.select_range",
        class: CommandClass::Read,
        summary: "Move the cursor a step and give the entries passed the opposite of the cursor entry's selection",
    },
    CommandSpec {
        id: "panel.set_cursor",
        class: CommandClass::Read,
        summary: "Put a panel's cursor on an entry, by index or name",
    },
    CommandSpec {
        id: "panel.set_view",
        class: CommandClass::Read,
        summary: "Show a panel's entries in the full view (columns), the brief view (names only), a column set, the comments view or the thumbnails",
    },
    CommandSpec {
        id: "panel.show_files",
        class: CommandClass::Read,
        summary: "Show a panel's files instead of its terminal; the shell keeps running",
    },
    CommandSpec {
        id: "panel.show_tree",
        class: CommandClass::Read,
        summary: "Show the folder tree of a panel's drive in its place, or its files again (Ctrl+F8); the window's other panel follows the tree's cursor",
    },
    CommandSpec {
        id: "panel.sort",
        class: CommandClass::Read,
        summary: "Sort a panel by name, extension, size or date; the same key again turns the direction around",
    },
    CommandSpec {
        id: "panel.switch_active",
        class: CommandClass::Read,
        summary: "Make the other panel, or a named one, the active panel",
    },
    CommandSpec {
        id: "panel.thumbnail_zoom",
        class: CommandClass::Write,
        summary: "Make the thumbnails' tiles a step larger or smaller (64 to 512 pixels); writes thumbnails.size",
    },
    CommandSpec {
        id: "panel.toggle_auto_columns",
        class: CommandClass::Write,
        summary: "Switch the automatic column sets by folder mask on or off; the choice is stored",
    },
    CommandSpec {
        id: "panel.toggle_comments",
        class: CommandClass::Read,
        summary: "Switch a panel between the comments view (name, extension, descript.ion comment) and the view before it",
    },
    CommandSpec {
        id: "panel.toggle_cursor_selection",
        class: CommandClass::Read,
        summary: "Select or deselect the entry under a panel's cursor, then move the cursor down",
    },
    CommandSpec {
        id: "panel.toggle_hidden",
        class: CommandClass::Read,
        summary: "Show or hide hidden and system entries in both panels; the choice is kept",
    },
    CommandSpec {
        id: "panel.toggle_selection",
        class: CommandClass::Read,
        summary: "Select or deselect entries of a panel by name",
    },
    CommandSpec {
        id: "panel.toggle_terminal",
        class: CommandClass::Exec,
        summary: "Switch a panel between its files and its terminal (a shell starts when none runs)",
    },
    CommandSpec {
        id: "panel.toggle_watch",
        class: CommandClass::Write,
        summary: "Switch the file watcher on or off: shown folders are read again when they change; the choice is stored",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "panel.get_state" => GetPanelState {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// What the panel knows of its folder's git status (RC-100-97); runs nothing.
            "panel.git_status" => GitStatus {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            "panel.list_dir" => ListDir {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, or relative to the panel's directory; `None` lists the panel's
                /// directory.
                #[serde(default)]
                path: Option<String>,
            },
            "panel.navigate" => Navigate {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, or relative to the panel's directory; `..` goes up.
                path: String,
            },
            "panel.set_cursor" => SetCursor {
                #[serde(default)]
                panel: Option<PanelId>,
                target: CursorTarget,
            },
            "panel.switch_active" => SwitchActivePanel {
                /// The panel to activate; `None` switches to the other one.
                #[serde(default)]
                panel: Option<PanelId>,
            },
            "panel.toggle_selection" => ToggleSelection {
                #[serde(default)]
                panel: Option<PanelId>,
                names: Vec<String>,
            },
            "panel.move_cursor" => MoveCursor {
                #[serde(default)]
                panel: Option<PanelId>,
                step: CursorStep,
            },
            /// A directory under the cursor is entered; a file stays where it is for now.
            "panel.open_entry" => OpenEntry {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// Insert in Total Commander: flip the cursor entry's selection, move one down.
            "panel.toggle_cursor_selection" => ToggleCursorSelection {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            "panel.toggle_terminal" => ToggleTerminal {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            "panel.refresh" => Refresh {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            "panel.select_all" => SelectAll {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            "panel.deselect_all" => DeselectAll {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            "panel.invert_selection" => InvertSelection {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            "panel.sort" => Sort {
                #[serde(default)]
                panel: Option<PanelId>,
                key: SortKey,
                /// `None`: the same key again turns the direction around, another key starts
                /// largest and newest first for size and date, from A for name and extension.
                #[serde(default)]
                descending: Option<bool>,
            },
            /// `full`, `brief`, `comments`, `thumbnails` (RC-100-89) or
            /// `{"custom": {"set": <id>}}` (RC-100-90), chosen by hand: it stays until the panel
            /// enters a folder of another set's mask.
            "panel.set_view" => SetListMode {
                #[serde(default)]
                panel: Option<PanelId>,
                mode: ListMode,
            },
            /// Ctrl+Shift+F2 (Total Commander's `cm_SrcComments`): the comments view, or back.
            "panel.toggle_comments" => ToggleComments {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// Total Commander's `cm_ToggleAutoViewModeSwitch`: `columns.auto_switch` switched.
            "panel.toggle_auto_columns" => ToggleAutoColumns {},
            /// Total Commander's `cm_SwitchWatchDirs`: `panels.watch` switched (RC-110-02).
            "panel.toggle_watch" => ToggleWatch {},
            /// Hidden and system entries in both panels; the choice is stored.
            "panel.toggle_hidden" => ToggleHidden {
                /// Show (`true`) or hide them; `None` switches.
                #[serde(default)]
                show: Option<bool>,
            },
            /// Num+ in Total Commander ("Select group"): the files a mask such as
            /// `*.txt;*.md|draft*` matches; directories only with a mask ending in `\`.
            "panel.select_by_mask" => SelectByMask(MaskRequest),
            /// Num- in Total Commander ("Unselect group"), with the same masks.
            "panel.deselect_by_mask" => DeselectByMask(MaskRequest),
            /// Alt+Num+ and Alt+Num-: every file with the extension of the one under the cursor.
            "panel.select_extension" => SelectExtension {
                #[serde(default)]
                panel: Option<PanelId>,
                select: bool,
            },
            /// Num /: the selection before the last file operation, as far as its names are still
            /// there.
            "panel.restore_selection" => RestoreSelection {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// Shift with a cursor key: the entries passed get the opposite of the cursor entry's
            /// state.
            "panel.select_range" => SelectRange {
                #[serde(default)]
                panel: Option<PanelId>,
                step: CursorStep,
            },
            /// Escape while a folder keeps the panel waiting (RC-080-04): every call on this
            /// machine's drives that waits now ends; the panel stays where it was.
            "panel.cancel_io" => CancelIo {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// The panel shows the other panel's folder when that panel is on the drive
            /// (`drives.follow_other_panel`), else the directory it last showed there, else the
            /// drive's root.
            "panel.change_drive" => ChangeDrive {
                #[serde(default)]
                panel: Option<PanelId>,
                /// A drive's id from `drives.list`, a drive letter (`c`, `C:`) or a label.
                drive: String,
            },
            /// Alt+F1 and Alt+F2: the window opens the panel's drive menu.
            "panel.drive_menu" => DriveMenu {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// The panel shows its files (again); its terminal keeps running.
            "panel.show_files" => ShowFiles {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// A new shell of the default profile in the panel's folder (RC-040-04).
            "panel.open_terminal_here" => OpenTerminalHere {
                /// The panel whose folder it starts in; the active one by default.
                #[serde(default)]
                panel: Option<PanelId>,
                /// Shown by the other panel instead of this one.
                #[serde(default)]
                other: bool,
            },
            /// The window opens the context menu of the panel's selection (RC-080-02).
            "panel.context_menu" => ContextMenu {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// Ctrl+F8 (RC-100-88): the panel shows the folder tree of its drive, or its files.
            "panel.show_tree" => ShowTree {
                #[serde(default)]
                panel: Option<PanelId>,
                /// The tree (`true`) or the files; `None` switches.
                #[serde(default)]
                show: Option<bool>,
            },
            /// Ctrl+Shift+F8 (RC-100-88): `panels.separate_tree` off, one, two, off.
            "panel.cycle_separate_tree" => CycleSeparateTree {},
            /// Ctrl and the mouse wheel over the thumbnails (RC-100-89): `thumbnails.size` a step
            /// up or down; no key, as in Total Commander.
            "panel.thumbnail_zoom" => ThumbnailZoom { step: crate::ZoomStep },
        } }
    };
}
pub(super) use commands;
