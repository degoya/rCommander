//! `Engine::dispatch` for the `file` group (`registry/file.rs`).

/// The `file` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::SplitFile(crate::SplitRequest {
                panel,
                path,
                part_size,
                target_dir,
            }) => {
                $self.split_file(panel, &path, part_size, target_dir.as_deref())
                    .await
            }
            Command::CombineFiles(crate::CombineRequest {
                panel,
                path,
                target_dir,
                target_name,
            }) => {
                $self.combine_files(panel, &path, target_dir.as_deref(), target_name.as_deref())
                    .await
            }
            Command::OpenFile { panel, path } => $self.open_file(panel, path.as_deref()).await,
        } }
    };
}
pub(crate) use arms;
