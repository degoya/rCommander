//! The menus of the menu bar, grouped as Total Commander groups its own (RC-080-18): File, Mark,
//! Commands, Net, View, Configuration, Start (Total Commander's Start menu at its place,
//! RC-100-86: "Change Start menu...", then the entries of `palette/user_commands.rs`), Workspaces
//! (RC-100-68: the workspaces first, `palette/workspaces.rs`, then its entries below), Help.
//! An entry is a row of the command palette by its name,
//! or a dialog without a row; its label is `menubar.<id>`, a menu's `menubar.title.<id>`, each
//! with one `&` before its mnemonic, and its key the keymap's under the same name. A name the
//! palette does not offer here (the network drives outside Windows) is left out when the menus
//! are built.
use super::{Dialog, Menu, Slot};
use crate::app::palette::lists::ListDialog;
use crate::app::settings::Page;

use Slot::{Entry, Separator};

/// Every menu, left to right.
pub const MENUS: &[Menu] = &[
    Menu {
        id: "file",
        slots: &[
            Entry("view_file"),
            Entry("view_file_alt"),
            Entry("edit_file"),
            Entry("new_text_file"),
            Separator,
            Entry("copy"),
            Entry("move"),
            Entry("rename"),
            Entry("mkdir"),
            Entry("delete"),
            Entry("delete_permanent"),
            Separator,
            Entry("archive.pack"),
            Entry("archive.extract"),
            Entry("archive.test"),
            Separator,
            Entry("file.properties"),
            Entry("file.create_link"),
            Entry("file.set_comment"),
            Entry("multi_rename.open_dialog"),
            Separator,
            Entry("checksum_create"),
            Entry("checksum_verify"),
            Entry("split_file"),
            Entry("combine_files"),
            Separator,
            // Total Commander's Files > "Internal associations" (RC-100-91).
            Entry("file.associations"),
            Separator,
            // F10 (RC-080-17); runs whatever `keys.f10` says, as the palette's row does.
            Entry("app.quit"),
        ],
    },
    Menu {
        id: "mark",
        slots: &[
            Entry("panel.select_by_mask"),
            Entry("panel.deselect_by_mask"),
            Entry("panel.select_all"),
            Entry("panel.deselect_all"),
            Entry("panel.invert_selection"),
            Entry("panel.restore_selection"),
            // RC-100-92: the selection saved and loaded, as Total Commander's Mark menu.
            Entry("panel.save_selection"),
            Slot::Dialog(
                "selection_to_file",
                Dialog::Lists(ListDialog::SelectionToFile),
            ),
            Slot::Dialog(
                "selection_from_file",
                Dialog::Lists(ListDialog::SelectionFromFile),
            ),
            Slot::Dialog(
                "selection_from_clipboard",
                Dialog::Lists(ListDialog::SelectionFromClipboard),
            ),
            Separator,
            Entry("select_extension"),
            Entry("deselect_extension"),
            Separator,
            Entry("compare.select_differences"),
            Separator,
            Entry("copy_full_paths"),
        ],
    },
    Menu {
        id: "commands",
        slots: &[
            Entry("files.find_dialog"),
            Entry("panel.branch_view"),
            Entry("files.disk_usage_dialog"),
            Entry("sync.open_dialog"),
            Entry("compare.by_content"),
            Separator,
            Entry("hotlist.list"),
            Entry("history.list"),
            Entry("panel.swap"),
            // RC-100-92: a list file into the panel, every other tab closed.
            Slot::Dialog("load_list", Dialog::Lists(ListDialog::LoadList)),
            Entry("tabs.close_all"),
            Separator,
            Entry("terminal.open"),
            Entry("panel.open_terminal_here"),
            Entry("panel.toggle_terminal"),
            Separator,
            Entry("queue.window"),
        ],
    },
    Menu {
        id: "net",
        slots: &[
            Entry("connections.manager"),
            Entry("connections.quick_connect"),
            Entry("disconnect"),
            Separator,
            Entry("map_network_drive"),
            Entry("unmap_network_drive"),
            Separator,
            Entry("wsl.refresh"),
        ],
    },
    Menu {
        id: "view",
        slots: &[
            Entry("view_brief"),
            Entry("view_full"),
            // The thumbnails (RC-100-89).
            Entry("view_thumbnails"),
            Separator,
            Entry("sort_name"),
            Entry("sort_ext"),
            Entry("sort_date"),
            Entry("sort_size"),
            Separator,
            Entry("panel.toggle_hidden"),
            // The file filters and the ignore list (RC-100-92), as Total Commander's Show menu.
            Entry("file_filter_all"),
            Entry("file_filter_programs"),
            Entry("panel.file_filter"),
            Entry("panel.toggle_ignore_list"),
            Entry("panel.toggle_quick_view"),
            // The folder trees (RC-100-88).
            Entry("panel.show_tree"),
            Entry("panel.cycle_separate_tree"),
            Entry("tree_dialog"),
            Entry("terminal.maximize"),
            Entry("center_divider"),
            Separator,
            Entry("panel.refresh"),
            // The bars of the window, shown or hidden (RC-100-87): Total Commander's Layout.
            Separator,
            Entry("toggle_bar.button_bar"),
            Entry("toggle_bar.vertical_button_bar"),
            // The comments view, the automatic switch and the column sets (RC-100-90).
            Separator,
            Entry("panel.toggle_comments"),
            Entry("panel.toggle_auto_columns"),
            Slot::Dialog("columns.configure", Dialog::ColumnSets),
        ],
    },
    Menu {
        id: "configuration",
        slots: &[
            Entry("settings"),
            Slot::Dialog("settings.host_keys", Dialog::Page(Page::HostKeys)),
            Slot::Dialog("settings.mcp", Dialog::Page(Page::Mcp)),
            // The settings on the Plugins page, tab Installed (RC-100-20); TC keeps plugins here too.
            Slot::Dialog("settings.plugins", Dialog::Page(Page::Plugins)),
            Entry("database"),
            Entry("config_export"),
            Entry("config_import"),
            Entry("vault.lock"),
            Separator,
            Slot::Dialog("import.total_commander", Dialog::TcImport),
            Slot::Dialog("import.remote_desktop_manager", Dialog::RdmImport),
            Slot::Dialog("import.ssh_config", Dialog::SshConfigImport),
            Separator,
            Entry("usercmd.manager"),
            Entry("button_bar.manager"),
        ],
    },
    Menu {
        id: "start",
        slots: &[Entry("start_menu.manager")],
    },
    Menu {
        id: "workspaces",
        slots: &[
            Slot::Dialog("workspaces.save_as", Dialog::SaveWorkspace),
            Entry("workspaces.manager"),
        ],
    },
    Menu {
        id: "help",
        // No command opens the wiki's keyboard reference; the palette lists every key.
        // "Check for updates..." opens the Updates page and checks (RC-100-05).
        slots: &[
            Slot::Dialog("palette", Dialog::Palette),
            Slot::Dialog("update.check", Dialog::Page(Page::Updates)),
            Entry("about"),
        ],
    },
];

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
