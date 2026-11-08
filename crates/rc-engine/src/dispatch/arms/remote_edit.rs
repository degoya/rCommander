//! `Engine::dispatch` for the `remote_edit` group (`registry/remote_edit.rs`).

/// The `remote_edit` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::CloseRemoteEdit { id } => $self.close_remote_edit(id).await,
            Command::ListRemoteEdits {} => Ok($self.list_remote_edits()),
            Command::SkipRemoteEdit { id } => $self.skip_remote_edit(id),
            Command::UploadRemoteEdit { id, choice, always } => {
                $self.upload_remote_edit(id, choice, always).await
            }
            Command::RemoteEditWindow {} => Ok($self.remote_edit_window()),
        } }
    };
}
pub(crate) use arms;
