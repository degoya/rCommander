//! Properties, attributes, links, occupied space, comments and names to the clipboard
//! (RC-020-06).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "clipboard.copy_names",
        class: CommandClass::Read,
        summary: "The names, full paths or details of the selected entries as text for the clipboard, one line each",
    },
    CommandSpec {
        id: "file.create_link",
        class: CommandClass::Write,
        summary: "Create a shortcut (.lnk), symbolic link, hard link or junction; never replaces an entry",
    },
    CommandSpec {
        id: "file.dir_size",
        class: CommandClass::Read,
        summary: "Occupied space of files and folders: bytes, files, folders, hard links counted once",
    },
    CommandSpec {
        id: "file.properties",
        class: CommandClass::Read,
        summary: "Properties of entries: kind, link target, sizes, times, attributes, owner, comment, sums",
    },
    CommandSpec {
        id: "file.set_attributes",
        class: CommandClass::Write,
        summary: "Change attributes, Unix mode and times of entries, optionally down their folders",
    },
    CommandSpec {
        id: "file.set_comment",
        class: CommandClass::Write,
        summary: "Set or remove an entry's comment in the folder's descript.ion",
    },
    CommandSpec {
        id: "panel.cancel_dir_sizes",
        class: CommandClass::Read,
        summary: "Stop counting folder sizes for a panel's size column",
    },
    CommandSpec {
        id: "panel.count_dir_content",
        class: CommandClass::Read,
        summary: "Count the size of every folder of a panel for its size column, in the background",
    },
    CommandSpec {
        id: "panel.select_and_size",
        class: CommandClass::Read,
        summary: "Space: flip the selection of the entry under the cursor and count it when it is a folder",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            // RC-020-06: properties, attributes, links, occupied space, comments, names.
            "clipboard.copy_names" => CopyNames {
                #[serde(default)]
                panel: Option<PanelId>,
                #[serde(default)]
                format: NameFormat,
            },
            /// Ctrl+Shift+F5: without `link` the window asks where the link goes.
            "file.create_link" => CreateLink {
                #[serde(default)]
                panel: Option<PanelId>,
                /// What the link leads to, absolute or relative; the entry under the cursor when
                /// `None`.
                #[serde(default)]
                target: Option<String>,
                /// Where the link goes, absolute or relative to the panel's folder.
                #[serde(default)]
                link: Option<String>,
                /// A shortcut where the backend makes them, else a symbolic link, when `None`.
                #[serde(default)]
                kind: Option<LinkKind>,
            },
            /// Ctrl+L: the window shows the occupied space of the selection.
            "file.dir_size" => DirSize {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, or relative; the selection, or the entry under the cursor, when empty.
                #[serde(default)]
                paths: Vec<String>,
            },
            /// Alt+Enter: the window shows the properties dialog.
            "file.properties" => GetProperties {
                #[serde(default)]
                panel: Option<PanelId>,
                /// As for `file.dir_size`.
                #[serde(default)]
                paths: Vec<String>,
                /// Count the folders' contents too.
                #[serde(default)]
                count: bool,
            },
            "file.set_attributes" => SetFileAttributes {
                #[serde(default)]
                panel: Option<PanelId>,
                /// As for `file.dir_size`.
                #[serde(default)]
                paths: Vec<String>,
                change: PropertyChange,
                /// Also everything below a folder that `apply_to` names.
                #[serde(default)]
                recursive: bool,
                #[serde(default)]
                apply_to: ApplyTo,
            },
            /// Ctrl+Z: without `comment` the window asks for it; an empty one removes it.
            "file.set_comment" => SetComment {
                #[serde(default)]
                panel: Option<PanelId>,
                /// The entry, absolute or relative; the one under the cursor when `None`.
                #[serde(default)]
                path: Option<String>,
                #[serde(default)]
                comment: Option<String>,
            },
            "panel.cancel_dir_sizes" => CancelDirSizes {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// Alt+Shift+Enter in Total Commander.
            "panel.count_dir_content" => CountDirContent {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            /// Space in Total Commander.
            "panel.select_and_size" => SelectAndSize {
                #[serde(default)]
                panel: Option<PanelId>,
            },
        } }
    };
}
pub(super) use commands;
