//! `Engine::dispatch` for the `import` group (`registry/import.rs`).

/// The `import` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ExportConfig(request) => $self.export_config(&request).await,
            Command::ImportConfig(request) => $self.import_config(&request).await,
            Command::ImportSshConfig(crate::SshConfigImportRequest {
                path,
                dry_run,
                hosts,
                folder_id,
            }) => {
                $self.import_ssh_config(path.as_deref(), dry_run, hosts.as_deref(), folder_id)
                    .await
            }
            Command::ImportRemoteDesktopManager(request) => $self.import_rdm(&request).await,
            Command::ImportTotalCommander(request) => $self.import_total_commander(&request).await,
            Command::ImportTotalCommanderPasswords(request) => {
                $self.import_total_commander_passwords(&request).await
            }
        } }
    };
}
pub(crate) use arms;
