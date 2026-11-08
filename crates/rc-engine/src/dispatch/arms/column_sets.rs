//! `Engine::dispatch` for the `column_sets` group (`registry/column_sets.rs`).

/// The `column_sets` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::CreateColumnSet { name, spec } => $self.create_column_set(&name, &spec).await,
            Command::DeleteColumnSet { id } => $self.delete_column_set(id).await,
            Command::GetColumnSet { id } => $self.get_column_set(id).await,
            Command::ListColumnSets {} => $self.list_column_sets().await,
            Command::MoveColumnSet { id, position } => $self.move_column_set(id, position).await,
            Command::RenameColumnSet { id, name } => {
                $self.update_column_set(id, Some(&name), None).await
            }
            Command::UpdateColumnSet { id, spec } => {
                $self.update_column_set(id, None, Some(&spec)).await
            }
        } }
    };
}
pub(crate) use arms;
