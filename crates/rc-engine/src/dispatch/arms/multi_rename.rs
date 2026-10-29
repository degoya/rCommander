//! `Engine::dispatch` for the `multi_rename` group (`registry/multi_rename.rs`).

/// The `multi_rename` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::MultiRenameApply(request) => $self.multi_rename_apply(&request).await,
            Command::MultiRenameOpenDialog { panel } => Ok($self.multi_rename_dialog(panel)),
            Command::MultiRenamePresetDelete { name } => $self.delete_rename_preset(&name).await,
            Command::MultiRenamePresetList => $self.rename_presets().await,
            Command::MultiRenamePresetRename { name, new_name } => {
                $self.rename_rename_preset(&name, &new_name).await
            }
            Command::MultiRenamePresetSave { name, rule } => {
                $self.save_rename_preset(&name, &rule).await
            }
            Command::MultiRenamePreview(request) => $self.multi_rename_preview(&request).await,
            Command::MultiRenameUndo => $self.multi_rename_undo().await,
        } }
    };
}
pub(crate) use arms;
