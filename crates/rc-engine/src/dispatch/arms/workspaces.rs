//! `Engine::dispatch` for the `workspaces` group (`registry/workspaces.rs`).

/// The `workspaces` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ApplyWorkspace { id } => $self.apply_workspace(id).await,
            Command::CaptureWorkspace { id, name } => $self.capture_workspace(id, name).await,
            Command::CreateWorkspace(request) => $self.create_workspace(request).await,
            Command::DeleteWorkspace { id } => $self.delete_workspace(id).await,
            Command::GetWorkspace { id } => $self.get_workspace(id).await,
            Command::ListWorkspaces {} => $self.list_workspaces().await,
            Command::WorkspaceManager {} => Ok($self.workspace_manager()),
            Command::MoveWorkspace { id, position } => $self.move_workspace(id, position).await,
            Command::RenameWorkspace { id, name } => $self.rename_workspace(id, name).await,
            Command::UpdateWorkspace(request) => $self.update_workspace(request).await,
        } }
    };
}
pub(crate) use arms;
