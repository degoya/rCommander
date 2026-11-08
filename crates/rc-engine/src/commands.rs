//! The commands' implementations, one module per group of the registry.

mod agent;
mod archive;
mod archive_ops;
mod certs;
mod checksum;
mod clipboard;
mod command_line;
mod comment;
mod compare;
mod compare_edit;
mod connections;
mod connections_duplicate;
mod connections_save;
mod connections_themes;
mod context_menu;
mod credentials;
mod cursor;
mod database;
mod denied_link;
mod devices;
mod diagnostics;
mod dir_count;
mod drives;
pub(crate) mod duplicates;
pub(crate) mod elevate;
mod exchange_connections;
mod exchange_export;
mod exchange_export_connections;
#[cfg(test)]
mod exchange_fixture;
mod exchange_import;
mod exchange_import_rows;
mod exchange_links;
mod exchange_places;
mod exchange_themes;
mod favourites;
pub(crate) mod file_filters;
pub(crate) mod file_ops;
mod file_ops_mask;
pub(crate) mod filter;
mod flat;
mod forwards;
mod hostkeys;
mod io_cancel;
#[cfg(test)]
mod jump_test_support;
mod list_file;
mod load_list;
mod login_scripts;
mod multi_rename;
pub(crate) mod multi_rename_presets;
mod multi_rename_undo;
mod network_drives;
mod open;
mod ops;
mod panel;
mod panel_listing;
mod portable;
mod properties;
mod queue;
mod queue_lists;
mod quit;
mod rdm_import;
mod recycle_bin;
mod remote;
pub(crate) mod search;
mod secrets;
mod selection;
mod selection_files;
mod sessions;
mod sessions_chain;
mod ssh_exec;
mod ssh_import;
mod ssh_import_links;
pub(crate) mod ssh_login;
mod ssh_terminal;
mod tc_import;
pub(crate) mod terminal;
mod terminal_image;
mod terminal_sync;
mod terminal_view;
mod terminal_watch;
mod thumbnails;
mod tree;
mod vault;
mod view;
mod viewer;
pub(crate) mod viewer_window;
mod wsl;

pub use archive_ops::default_archive_name;
pub(crate) use comment::read_descriptions;
pub use credentials::SHARED_CREDENTIALS_KEY;
pub(crate) use credentials::credential_needs_gui;
pub use hostkeys::{ImportConflict, ImportReport};
pub use multi_rename::MultiRenameRequest;
pub use multi_rename_presets::{MAX_PRESET_NAME, MAX_PRESETS, RenamePreset};
pub(crate) use panel::resolve_path;
pub use queue_lists::{SavedList, Unloadable};
pub use search::{DEFAULT_PAGE, MAX_PAGE, SearchHandle};
pub(crate) use secrets::{read_beside_certificate, read_local_key_file};
pub use ssh_exec::{MAX_EXEC_COMMAND, MAX_EXEC_STDIN};
pub use ssh_login::{LoginRunState, LoginScriptRun};
pub use ssh_terminal::{SSH_SHELL, SSH_TERM, cd_command, shell_quote};
pub use terminal_sync::PanelTerminal;

impl crate::Outcome {
    /// The outcome naming one job.
    pub(crate) fn job(snapshot: rc_ops::JobSnapshot) -> Self {
        Self::Job(Box::new(snapshot))
    }
}
