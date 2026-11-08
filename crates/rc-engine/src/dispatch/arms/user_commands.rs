//! `Engine::dispatch` for the `user_commands` group (`registry/user_commands.rs`).

/// The `user_commands` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::DeleteStartItem { id } => $self.delete_start_item(id).await,
            Command::ListStartMenu {} => $self.list_start_menu().await,
            Command::StartMenuManager {} => Ok($self.user_commands_window(true)),
            Command::MoveStartItem {
                id,
                parent,
                position,
            } => $self.move_start_item(id, parent, position).await,
            Command::RunStartItem { id, panel, params } => {
                $self.run_start_item(id, panel, params).await
            }
            Command::SaveStartItem { id, parent, item } => {
                $self.save_start_item(id, parent, item).await
            }
            Command::DeleteUserCommand { id } => $self.delete_user_command(id).await,
            Command::ListUserCommands {} => $self.list_user_commands().await,
            Command::UserCommandManager {} => Ok($self.user_commands_window(false)),
            Command::RunUserCommand {
                name,
                panel,
                params,
            } => $self.run_user_command(&name, panel, params).await,
            Command::SaveUserCommand { id, command } => {
                $self.save_user_command(id, command).await
            }
        } }
    };
}
pub(crate) use arms;
