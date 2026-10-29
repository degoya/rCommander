//! `Engine::dispatch` for the `diagnostics` group (`registry/diagnostics.rs`).

/// The `diagnostics` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ErrorLog(crate::ErrorLogRequest {
                level,
                contains,
                limit,
            }) => $self.error_log(level, contains, limit).await,
            Command::ClearErrorLog => $self.clear_error_log().await,
            Command::GetRetention => $self.retention().await,
            Command::SetRetention(crate::RetentionRequest {
                table,
                max_rows,
                max_age_days,
            }) => $self.set_retention(&table, max_rows, max_age_days).await,
        } }
    };
}
pub(crate) use arms;
