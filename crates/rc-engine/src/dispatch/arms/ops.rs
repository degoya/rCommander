//! `Engine::dispatch` for the `ops` group (`registry/ops.rs`).

/// The `ops` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::OpsAnswer { job, answer } => {
                Ok(Outcome::job($self.inner.ops.answer(job, answer)?))
            }
            Command::OpsCancel { job } => Ok(Outcome::job($self.inner.ops.cancel(job)?)),
            Command::OpsCopy(crate::TransferRequest {
                panel,
                sources,
                target,
                options,
            }) => {
                $self.ops_transfer(panel, &sources, target.as_deref(), options, false)
                    .await
            }
            Command::OpsDelete(crate::DeleteRequest {
                panel,
                sources,
                mode,
                on_error,
                queue,
            }) => {
                $self.ops_delete(panel, &sources, (mode, on_error), queue)
                    .await
            }
            Command::OpsList { job } => $self.ops_list(job),
            Command::OpsMkdir { panel, path } => $self.ops_mkdir(panel, &path).await,
            Command::OpsMove(crate::TransferRequest {
                panel,
                sources,
                target,
                options,
            }) => {
                $self.ops_transfer(panel, &sources, target.as_deref(), options, true)
                    .await
            }
            Command::OpsPause { job } => Ok(Outcome::job($self.inner.ops.pause(job)?)),
            Command::OpsResume { job } => Ok(Outcome::job($self.inner.ops.resume(job)?)),
        } }
    };
}
pub(crate) use arms;
