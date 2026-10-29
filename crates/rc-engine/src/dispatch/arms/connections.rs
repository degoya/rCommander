//! `Engine::dispatch` for the `connections` group (`registry/connections.rs`).

/// The `connections` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ListConnections => $self.list_connections().await,
            Command::GetConnection { id } => $self.get_connection(id).await,
            Command::CreateConnection {
                folder_id,
                connection,
            } => $self.create_connection(folder_id, &connection).await,
            Command::SaveConnection(request) => $self.save_connection(&request).await,
            Command::UpdateConnection { id, patch } => $self.update_connection(id, &patch).await,
            Command::MoveConnection(crate::MoveConnectionRequest {
                id,
                folder_id,
                sort_order,
            }) => $self.move_connection(id, folder_id, sort_order).await,
            Command::DeleteConnection { id } => $self.delete_connection(id).await,
            Command::DuplicateConnection { id } => $self.duplicate_connection(id).await,
            Command::CreateFolder { parent_id, name } => {
                $self.create_folder(parent_id, &name).await
            }
            Command::RenameFolder { id, name } => $self.rename_folder(id, &name).await,
            Command::MoveFolder(crate::MoveFolderRequest {
                id,
                parent_id,
                sort_order,
            }) => $self.move_folder(id, parent_id, sort_order).await,
            Command::DeleteFolder { id, recursive } => $self.delete_folder(id, recursive).await,
            Command::SaveCredential(request) => $self.save_credential(&request).await,
            Command::MoveCredential { id, folder_id } => $self.move_credential(id, folder_id).await,
            Command::DeleteCredential { id } => $self.delete_credential(id).await,
        } }
    };
}
pub(crate) use arms;
