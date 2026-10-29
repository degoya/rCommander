//! `Engine::dispatch` for the `tabs` group (`registry/tabs.rs`).

/// The `tabs` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ListTabs(request) => Ok($self.list_tabs(request.panel)),
            Command::OpenTab(request) => $self.open_tab(request).await,
            Command::CloseTab(request) => $self.close_tab(request).await,
            Command::SelectTab(request) => $self.select_tab(request.panel, request.tab).await,
            Command::NextTab(request) => $self.next_tab(request).await,
            Command::LockTab(request) => $self.lock_tab(request),
            Command::CopyTabToOther(request) => $self.copy_tab_to_other(request),
            Command::MoveTab(request) => $self.move_tab(request),
            Command::SwapPanels(request) => Ok($self.swap_panels(request.with_tabs)),
            Command::TransferDir(request) => $self.transfer_dir(request).await,
        } }
    };
}
pub(crate) use arms;
