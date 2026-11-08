//! `Engine::dispatch` for the `panel_lists` group (`registry/panel_lists.rs`).

/// The `panel_lists` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::SetFileFilter { panel, filter } => {
                $self.set_file_filter(panel, filter.unwrap_or_default()).await
            }
            Command::ListFileFilters => $self.list_file_filters().await,
            Command::SaveFileFilter { name, mask } => $self.save_file_filter(&name, &mask).await,
            Command::RenameFileFilter { name, new_name } => {
                $self.rename_file_filter(&name, &new_name).await
            }
            Command::DeleteFileFilter { name } => $self.delete_file_filter(&name).await,
            Command::ToggleIgnoreList { enabled } => $self.toggle_ignore_list(enabled).await,
            Command::SaveSelection { panel } => Ok($self.save_selection(panel)),
            Command::SelectionToFile {
                panel,
                path,
                overwrite,
            } => $self.selection_to_file(panel, &path, overwrite).await,
            Command::SelectionFromFile { panel, path } => {
                $self.selection_from_file(panel, &path).await
            }
            Command::SelectionFromText { panel, text } => $self.selection_from_text(panel, &text),
            Command::LoadList { panel, path } => $self.load_list(panel, &path).await,
        } }
    };
}
pub(crate) use arms;
