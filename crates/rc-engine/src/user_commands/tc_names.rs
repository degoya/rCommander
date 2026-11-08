//! Total Commander's internal command names (`cm_*`) and the keymap rows rCommander runs for
//! them (RC-100-86): an own command or a Start menu entry imported from `usercmd.ini` or
//! `wincmd.ini`, or typed as Total Commander users write them, may name `cm_Copy` where
//! rCommander's row is `copy`. Total Commander compares the names without regard to case, and so
//! does [`row_of`]. A name missing here is no error until it runs: the import keeps it with a
//! note, and running it answers `user_command_unknown_row`.

/// Each Total Commander name in lower case with the keymap row (`keymap/default.json`) that does
/// what it does, sorted by the name.
pub const TC_NAMES: &[(&str, &str)] = &[
    ("cm_addfilenametocmdline", "cmdline_insert_name"),
    ("cm_addpathandfilenametocmdline", "cmdline_insert_path"),
    ("cm_addpathtocmdline", "cmdline_insert_folder"),
    ("cm_calculatespace", "file.dir_size"),
    ("cm_clearall", "panel.deselect_all"),
    ("cm_closecurrenttab", "tabs.close"),
    ("cm_combinefiles", "combine_files"),
    ("cm_comparedirs", "compare.select_differences"),
    ("cm_comparefilesbycontent", "compare.by_content"),
    ("cm_contextmenu", "panel.context_menu"),
    ("cm_copy", "copy"),
    ("cm_copyfullnamestoclip", "copy_full_paths"),
    ("cm_copysamepanel", "copy_here"),
    ("cm_copytoclipboard", "clipboard_copy_files"),
    ("cm_countdircontent", "panel.count_dir_content"),
    ("cm_createchecksums", "checksum_create"),
    ("cm_cuttoclipboard", "clipboard_cut_files"),
    ("cm_delete", "delete"),
    ("cm_dirbranch", "panel.branch_view"),
    ("cm_dirbranchsel", "branch_view_selection"),
    ("cm_directoryhistory", "history.list"),
    ("cm_directoryhotlist", "hotlist.list"),
    ("cm_edit", "edit_file"),
    ("cm_editcomment", "file.set_comment"),
    ("cm_editnewfile", "new_text_file"),
    ("cm_exchange", "panel.swap"),
    ("cm_exchangeselection", "panel.invert_selection"),
    ("cm_exchangewithtabs", "swap_with_tabs"),
    ("cm_exit", "app.quit"),
    ("cm_ftpconnect", "connections.manager"),
    ("cm_ftpdisconnect", "disconnect"),
    ("cm_ftpnew", "connections.quick_connect"),
    ("cm_gotonextdir", "history.forward"),
    ("cm_gotoparent", "parent_folder"),
    ("cm_gotopreviousdir", "history.back"),
    ("cm_gotoroot", "root_folder"),
    ("cm_leftchangedrive", "drive_menu_left"),
    ("cm_list", "view_file"),
    ("cm_mkdir", "mkdir"),
    ("cm_multirenamefiles", "multi_rename.open_dialog"),
    ("cm_opendirinnewtab", "tab_open_cursor"),
    ("cm_opendirinnewtabother", "tab_open_cursor_other"),
    ("cm_opennewtab", "tabs.open"),
    ("cm_packfiles", "archive.pack"),
    ("cm_pastefromclipboard", "clipboard_paste_files"),
    ("cm_properties", "file.properties"),
    ("cm_quickfilter", "panel.filter"),
    ("cm_renameonly", "rename"),
    ("cm_renmov", "move"),
    ("cm_rereadsource", "panel.refresh"),
    ("cm_restoreselection", "panel.restore_selection"),
    ("cm_rightchangedrive", "drive_menu_right"),
    ("cm_searchfor", "files.find_dialog"),
    ("cm_selectall", "panel.select_all"),
    ("cm_selectcurrentextension", "select_extension"),
    ("cm_showcmdlinehistory", "cmdline_history"),
    ("cm_shrinkselection", "panel.deselect_by_mask"),
    ("cm_splitfile", "split_file"),
    ("cm_spreadselection", "panel.select_by_mask"),
    ("cm_srcbrief", "view_brief"),
    ("cm_srcbydatetime", "sort_date"),
    ("cm_srcbyext", "sort_ext"),
    ("cm_srcbyname", "sort_name"),
    ("cm_srcbysize", "sort_size"),
    ("cm_srcfull", "view_full"),
    ("cm_srcquickview", "panel.toggle_quick_view"),
    ("cm_switchtonexttab", "tabs.next"),
    ("cm_switchtoprevioustab", "tab_previous"),
    ("cm_syncdirs", "sync.open_dialog"),
    ("cm_testarchive", "archive.test"),
    ("cm_transferleft", "transfer_left"),
    ("cm_transferright", "transfer_right"),
    ("cm_unpackfiles", "archive.extract"),
    ("cm_unselectcurrentextension", "deselect_extension"),
    ("cm_verifychecksums", "checksum_verify"),
];

/// The keymap row Total Commander's `name` (`cm_Copy`, any case) runs in rCommander; `None` for
/// a name that is no `cm_*` or one rCommander does not know.
#[must_use]
pub fn row_of(name: &str) -> Option<&'static str> {
    let lower = name.trim().to_ascii_lowercase();
    TC_NAMES
        .binary_search_by(|(tc, _)| (*tc).cmp(lower.as_str()))
        .ok()
        .map(|at| TC_NAMES[at].1)
}

#[cfg(test)]
#[path = "tc_names_tests.rs"]
mod tests;
