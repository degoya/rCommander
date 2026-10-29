//! `Engine::dispatch` for the `secrets` group (`registry/secrets.rs`).

/// The `secrets` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ImportKey(crate::ImportKeyRequest {
                connection_id,
                path,
                passphrase,
            }) => {
                $self.import_key(connection_id, &path, passphrase.as_ref())
                    .await
            }
            Command::MapSecretSource(reference) => $self.map_secret_source(reference).await,
            Command::SearchSecretSource { source, query } => {
                $self.search_secret_source(&source, &query).await
            }
            Command::UnmapSecretSource {
                connection_id,
                kind,
            } => $self.unmap_secret_source(connection_id, kind).await,
            Command::ListSecretSources { connection_id } => {
                $self.list_secret_sources(connection_id).await
            }
        } }
    };
}
pub(crate) use arms;
