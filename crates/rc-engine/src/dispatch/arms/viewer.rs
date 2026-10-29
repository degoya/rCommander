//! `Engine::dispatch` for the `viewer` group (`registry/viewer.rs`).

/// The `viewer` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ReadHex(request) => $self.read_hex(request).await,
            Command::ReadText(request) => $self.read_text(request).await,
            Command::ViewFile { panel, path } => $self.view_file(panel, path.as_deref()).await,
            Command::ToggleQuickView { panel } => Ok($self.toggle_quick_view(panel)),
        } }
    };
}
pub(crate) use arms;
