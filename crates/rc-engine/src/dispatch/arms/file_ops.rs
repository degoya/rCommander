//! `Engine::dispatch` for the `file_ops` group (`registry/file_ops.rs`).

/// The `file_ops` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::FileDialog { panel, dialog } => Ok($self.file_dialog(panel, dialog).await),
            Command::OpsCopyTo {
                panel,
                sources,
                target,
                options,
                queue,
            } => {
                $self.ops_transfer_to(panel, &sources, &target, (options, queue), false)
                    .await
            }
            Command::OpsMoveTo {
                panel,
                sources,
                target,
                options,
                queue,
            } => {
                $self.ops_transfer_to(panel, &sources, &target, (options, queue), true)
                    .await
            }
            Command::OpenExternal {
                panel,
                path,
                program,
                create,
                cursor_only,
            } => {
                $self.open_external(panel, path.as_deref(), program, (create, cursor_only))
                    .await
            }
        } }
    };
}
pub(crate) use arms;
