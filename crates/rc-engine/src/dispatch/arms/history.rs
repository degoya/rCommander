//! `Engine::dispatch` for the `history` group (`registry/history.rs`).

/// The `history` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::HistoryBack(request) => $self.history_step(request.panel, -1).await,
            Command::HistoryForward(request) => $self.history_step(request.panel, 1).await,
            Command::HistoryList(request) => Ok($self.history_list(request.panel)),
            Command::HistoryJump(request) => $self.history_jump(request).await,
        } }
    };
}
pub(crate) use arms;
