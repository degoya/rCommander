//! `Engine::dispatch` for the `forwards` group (`registry/forwards.rs`, RC-110-03).

/// The `forwards` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::DeleteForward { id } => $self.delete_forward(id).await,
            Command::ListForwards { connection_id } => $self.list_forwards(connection_id).await,
            Command::SaveForward(request) => {
                $self
                    .save_forward(request.connection_id, request.id, &request.forward)
                    .await
            }
            Command::StartForward { id } => $self.start_forward(id).await,
            Command::StopForward { id } => Ok($self.stop_forward(id)),
            Command::StopAllForwards { connection_id } => {
                Ok($self.stop_all_forwards(connection_id))
            }
        } }
    };
}
pub(crate) use arms;
