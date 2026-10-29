//! `Engine::dispatch` for the `app` group (`registry/app.rs`).

/// The `app` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::About {} => Ok(Outcome::About(crate::about::about())),
            Command::Quit {} => Ok($self.quit().await),
        } }
    };
}
pub(crate) use arms;
