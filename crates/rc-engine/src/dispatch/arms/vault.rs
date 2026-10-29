//! `Engine::dispatch` for the `vault` group (`registry/vault.rs`).

/// The `vault` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::GetVaultStatus => Ok($self.vault_status()),
            Command::LockVault => Ok($self.lock_vault()),
            Command::CreateVault { password } => $self.create_vault(&password).await,
            Command::UnlockVault { password } => $self.unlock_vault(&password).await,
            Command::ChangeMasterPassword { current, new } => {
                $self.change_master_password(&current, &new).await
            }
            Command::RemoveVault => $self.remove_vault().await,
        } }
    };
}
pub(crate) use arms;
