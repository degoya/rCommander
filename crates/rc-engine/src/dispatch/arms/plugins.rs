//! `Engine::dispatch` for the `plugins` group (`registry/plugins.rs`).

/// The `plugins` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::DisablePlugin { id } => $self.enable_plugin(&id, false).await,
            Command::EnablePlugin { id } => $self.enable_plugin(&id, true).await,
            Command::InstallPlugin {
                path,
                fingerprint,
                digest,
            } => {
                $self
                    .install_plugin(&path, fingerprint.as_deref(), &digest)
                    .await
            }
            Command::ListPlugins { id } => $self.list_plugins(id.as_deref()).await,
            Command::PreviewPlugin { path } => $self.preview_plugin(&path).await,
            Command::RemovePlugin { id, version } => {
                $self.remove_plugin_version(&id, &version).await
            }
            Command::ResumePlugin { id } => $self.resume_plugin(&id).await,
            Command::RollbackPlugin { id } => $self.rollback_plugin(&id).await,
        } }
    };
}
pub(crate) use arms;
