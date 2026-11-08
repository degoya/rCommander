//! `Engine::dispatch` for the `button_bars` group (`registry/button_bars.rs`).

/// The `button_bars` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::DeleteButtonBar { id } => $self.delete_button_bar(id).await,
            Command::FocusButtonBar { bar } => Ok($self.focus_button_bar(bar)),
            Command::GetButtonBar { id } => $self.get_button_bar(id).await,
            Command::ImportButtonBar { path } => $self.import_button_bar(&path).await,
            Command::PreviewButtonBarImport { path } => {
                $self.preview_button_bar_import(&path).await
            }
            Command::ListButtonBars {} => $self.list_button_bars().await,
            Command::ButtonBarManager {} => Ok($self.button_bar_window()),
            Command::RenameButtonBar { id, name } => $self.rename_button_bar(id, &name).await,
            Command::RunButton {
                bar,
                button,
                panel,
                params,
                files,
            } => $self.run_button(bar, button, panel, params, files).await,
            Command::SaveButtonBar { id, name, spec } => {
                $self.save_button_bar(id, &name, &spec).await
            }
            Command::ToggleBar { bar } => $self.toggle_bar(bar).await,
        } }
    };
}
pub(crate) use arms;
