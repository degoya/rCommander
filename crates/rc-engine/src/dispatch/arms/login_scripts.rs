//! `Engine::dispatch` for the `login_scripts` group (`registry/login_scripts.rs`).

/// The `login_scripts` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ListLoginScripts => $self.list_login_scripts().await,
            Command::GetLoginScript { id } => $self.get_login_script(id).await,
            Command::CreateLoginScript { script } => $self.create_login_script(&script).await,
            Command::UpdateLoginScript { id, patch } => $self.update_login_script(id, &patch).await,
            Command::DeleteLoginScript { id } => $self.delete_login_script(id).await,
            Command::AssignLoginScript {
                connection_id,
                script_id,
            } => $self.assign_login_script(connection_id, script_id).await,
            Command::SetLoginScriptSecret {
                script_id,
                label,
                value,
            } => {
                $self.set_login_script_secret(script_id, &label, &value)
                    .await
            }
            Command::ClearLoginScriptSecret {
                script_id,
                secret_id,
            } => $self.clear_login_script_secret(script_id, secret_id).await,
        } }
    };
}
pub(crate) use arms;
