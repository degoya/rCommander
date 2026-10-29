//! `Engine::dispatch` for the `archive_ops` group (`registry/archive_ops.rs`).

/// The `archive_ops` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ArchiveExtract {
                panel,
                archives,
                target,
                settings,
                options,
            } => {
                $self.archive_extract(panel, &archives, target.as_deref(), settings, options).await
            }
            Command::ArchivePack {
                panel,
                sources,
                target,
                settings,
                options,
            } => {
                $self.archive_pack(panel, &sources, target.as_deref(), settings, options).await
            }
            Command::ArchivePassword { job, password } => {
                $self.archive_password(job, password.as_ref())
            }
        } }
    };
}
pub(crate) use arms;
