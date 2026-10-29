//! `Engine::dispatch` for the `search` group (`registry/search.rs`).

/// The `search` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::SearchFiles(crate::SearchRequest {
                panel,
                paths,
                query,
                limit,
                cursor,
            }) => {
                $self.search_files(panel, &paths, &query, limit, cursor.as_deref())
                    .await
            }
            Command::BranchView {
                panel,
                selection,
                show,
            } => $self.branch_view(panel, selection, show).await,
            Command::FeedPaths { panel, root, paths } => {
                $self.feed_paths(panel, root.as_deref(), &paths).await
            }
            Command::FindFilesDialog { panel } => Ok($self.find_dialog(panel)),
        } }
    };
}
pub(crate) use arms;
