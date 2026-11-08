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
            Command::FindDuplicates(crate::DuplicatesRequest {
                panel,
                paths,
                query,
                duplicates,
                limit,
                cursor,
            }) => {
                $self
                    .find_duplicates(panel, &paths, query, duplicates, limit, cursor.as_deref())
                    .await
            }
            Command::FeedPaths {
                panel,
                root,
                paths,
                groups,
            } => {
                $self
                    .feed_paths(panel, root.as_deref(), &paths, &groups)
                    .await
            }
            Command::SelectDuplicates { panel, spec } => $self.select_duplicates(panel, &spec),
            Command::FindFilesDialog { panel } => Ok($self.find_dialog(panel)),
        } }
    };
}
pub(crate) use arms;
