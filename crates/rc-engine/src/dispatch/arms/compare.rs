//! `Engine::dispatch` for the `compare` group (`registry/compare.rs`).

/// The `compare` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::CompareFiles(request) => $self.compare_files(request).await,
            Command::CompareDirs(request) => $self.compare_dirs(request).await,
            Command::OpenDiff {} => $self.open_diff(),
            Command::SelectDifferences { by_content } => $self.select_differences(by_content).await,
            Command::SyncApply(request) => $self.sync_apply(request).await,
            Command::OpenSyncDialog {} => Ok($self.open_sync_dialog()),
        } }
    };
}
pub(crate) use arms;
