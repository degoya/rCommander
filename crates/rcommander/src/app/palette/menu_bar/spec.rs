//! The menus of the menu bar, grouped as Total Commander groups its own (RC-080-18): File, Mark,
//! Commands, Net, View, Configuration, Help. An entry is a row of the command palette by its name,
//! or a dialog without a row; its label is `menubar.<id>`, a menu's `menubar.title.<id>`, each
//! with one `&` before its mnemonic, and its key the keymap's under the same name. A name the
//! palette does not offer here (the network drives outside Windows) is left out when the menus
//! are built.

use super::{Dialog, Menu, Slot};
use crate::app::settings::Page;

use Slot::{Entry, Separator};

/// Every menu, left to right.
pub const MENUS: &[Menu] = &[
    Menu {
        id: "file",
        slots: &[
            Entry("view_file"),
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
            Entry("sync.open_dialog"),
            Entry("compare.open_diff"),
            Separator,
            Entry("hotlist.list"),
            Entry("history.list"),
            Entry("panel.swap"),
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
            Separator,
            Entry("sort_name"),
            Entry("sort_ext"),
            Entry("sort_date"),
            Entry("sort_size"),
            Separator,
            Entry("panel.toggle_hidden"),
            Entry("panel.toggle_quick_view"),
            Entry("terminal.maximize"),
            Entry("center_divider"),
            Separator,
            Entry("panel.refresh"),
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
