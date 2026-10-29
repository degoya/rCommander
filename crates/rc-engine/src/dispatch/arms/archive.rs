//! `Engine::dispatch` for the `archive` group (`registry/archive.rs`).

/// The `archive` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ArchiveList { panel, path, dir } => {
                $self.archive_list(panel, path.as_deref(), dir.as_deref())
                    .await
            }
            Command::ArchiveTest { panel, paths } => $self.archive_test(panel, &paths).await,
        } }
    };
}
pub(crate) use arms;
