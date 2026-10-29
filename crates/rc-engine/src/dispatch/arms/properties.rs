//! `Engine::dispatch` for the `properties` group (`registry/properties.rs`).

/// The `properties` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::CopyNames { panel, format } => $self.copy_names(panel, format).await,
            Command::CreateLink {
                panel,
                target,
                link,
                kind,
            } => {
                $self.create_link(panel, target.as_deref(), link.as_deref(), kind)
                    .await
            }
            Command::DirSize { panel, paths } => $self.dir_size(panel, &paths).await,
            Command::GetProperties {
                panel,
                paths,
                count,
            } => $self.properties(panel, &paths, count).await,
            Command::SetFileAttributes {
                panel,
                paths,
                change,
                recursive,
                apply_to,
            } => {
                $self.set_file_attributes(panel, &paths, change, recursive, apply_to)
                    .await
            }
            Command::SetComment {
                panel,
                path,
                comment,
            } => {
                $self.set_comment(panel, path.as_deref(), comment.as_deref())
                    .await
            }
            Command::CancelDirSizes { panel } => Ok($self.cancel_dir_sizes(panel)),
            Command::CountDirContent { panel } => Ok($self.count_dir_content(panel)),
            Command::SelectAndSize { panel } => Ok($self.select_and_size(panel).await),
        } }
    };
}
pub(crate) use arms;
