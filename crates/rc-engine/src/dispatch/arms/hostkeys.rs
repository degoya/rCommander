//! `Engine::dispatch` for the `hostkeys` group (`registry/hostkeys.rs`).

/// The `hostkeys` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::AnswerHostKey { question, answer } => {
                $self.answer_host_key(question, answer).await
            }
            Command::ImportKnownHosts { path } => $self.import_host_keys(path.as_deref()).await,
            Command::ListHostKeys { filter } => $self.list_host_keys(filter.as_deref()).await,
            Command::RemoveHostKey { id } => $self.remove_host_key(id).await,
            Command::ReplaceHostKey { question } => $self.replace_host_key(question).await,
        } }
    };
}
pub(crate) use arms;
