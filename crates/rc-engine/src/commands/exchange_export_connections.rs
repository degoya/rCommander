//! The connections part of `config.export` (RC-080-19): every connection with its mark, its
//! folders, its login script and - with secrets - its secrets; since RC-110-03 its jump host, by
//! the mark of the jump host's own entry, and its local port forwards. Moved out of
//! `exchange_export.rs`, which keeps the file and the other parts.

use std::collections::HashMap;

use rc_db::{Db, connections::ConnectionOrigin};
use rc_vault::{ExposeSecret as _, SecretString};

use super::exchange_export::{draft_of, flatten};
use crate::{
    Engine, EngineError, LoginScriptInfo, SecretKind, SecretRef,
    exchange::{
        ConnectionsPart, ExportSecret, ExportedConnection, ExportedForward, ExportedMark,
        ExportedScript, ExportedScriptSecret, ExportedSource, SOURCE_PREFIX,
    },
};

impl Engine {
    /// The connection tree, its login scripts and - with `secrets` - their secrets.
    pub(super) async fn export_connections(
        &self,
        db: &Db,
        origin: &str,
        secrets: bool,
    ) -> Result<ConnectionsPart, EngineError> {
        let tree = db.connection_tree().await?;
        let mut listed = Vec::new();
        for connection in tree.connections {
            listed.push((Vec::new(), connection));
        }
        flatten(&tree.folders, &[], &mut listed);
        let marks: HashMap<_, ConnectionOrigin> = db
            .connection_origins()
            .await?
            .into_iter()
            .map(|mark| (mark.id, mark))
            .collect();
        let scripts = db
            .login_scripts()
            .await?
            .into_iter()
            .map(LoginScriptInfo::from_record)
            .collect::<Result<Vec<_>, _>>()?;
        let names: HashMap<i64, String> = scripts
            .iter()
            .map(|script| (script.id.0, script.name.clone()))
            .collect();
        let stored = listed.iter().any(|(_, connection)| {
            let flags = connection.secrets;
            flags.has_password || flags.has_key || flags.has_passphrase
        }) || scripts.iter().any(|script| !script.secrets.is_empty());
        if secrets && stored {
            // Locked or absent is answered before anything is read.
            self.vault_handle().await?.touch()?;
        }
        // Every connection's mark first: a jump host travels as its entry's (RC-110-03).
        let exported: HashMap<_, ExportedMark> = listed
            .iter()
            .map(|(_, connection)| {
                let (source, key) = match marks.get(&connection.id) {
                    Some(ConnectionOrigin {
                        source: Some(source),
                        key: Some(key),
                        ..
                    }) if source.starts_with(SOURCE_PREFIX) => (source.clone(), key.clone()),
                    _ => (
                        format!("{SOURCE_PREFIX}{origin}"),
                        connection.id.to_string(),
                    ),
                };
                (connection.id, ExportedMark { source, key })
            })
            .collect();
        let mut part = ConnectionsPart::default();
        for (folders, connection) in listed {
            let ExportedMark { source, key } = exported[&connection.id].clone();
            let login_script = connection
                .login_script_id
                .and_then(|id| names.get(&id).cloned());
            let mut entry = ExportedConnection {
                source,
                key,
                folders,
                connection: draft_of(&connection),
                login_script,
                password: None,
                private_key: None,
                passphrase: None,
                sources: db
                    .secret_sources(connection.id)
                    .await?
                    .into_iter()
                    .map(ExportedSource::from)
                    .collect(),
                jump: connection
                    .jump_id
                    .and_then(|jump| exported.get(&jump).cloned()),
                forwards: db
                    .forwards(connection.id)
                    .await?
                    .into_iter()
                    .map(|forward| ExportedForward {
                        bind_address: forward.bind_address,
                        bind_port: forward.bind_port,
                        target_host: forward.target_host,
                        target_port: forward.target_port,
                        auto_start: forward.auto_start,
                        name: forward.name,
                    })
                    .collect(),
            };
            if secrets {
                for (kind, slot) in [
                    (SecretKind::Password, &mut entry.password),
                    (SecretKind::Key, &mut entry.private_key),
                    (SecretKind::Passphrase, &mut entry.passphrase),
                ] {
                    if connection.secrets.has(kind) {
                        *slot = self.export_secret(connection.id, kind).await?;
                    }
                }
            }
            part.connections.push(entry);
        }
        for script in scripts {
            let mut labels = Vec::with_capacity(script.secrets.len());
            for label in &script.secrets {
                let value = if secrets {
                    self.resolve_script_secret(script.id, label.id)
                        .await?
                        .map(|value| ExportSecret::new(value.expose_secret()))
                } else {
                    None
                };
                labels.push(ExportedScriptSecret {
                    id: label.id,
                    label: label.label.clone(),
                    value,
                });
            }
            part.login_scripts.push(ExportedScript {
                name: script.name,
                steps: script.steps,
                step_timeout_ms: script.step_timeout_ms,
                total_timeout_ms: script.total_timeout_ms,
                secrets: labels,
            });
        }
        Ok(part)
    }

    /// The stored secret `kind` of `connection`, decrypted - from the vault, never from a source
    /// the secret is mapped to (RC-100-34): the reference travels instead.
    async fn export_secret(
        &self,
        connection: crate::ConnectionId,
        kind: SecretKind,
    ) -> Result<Option<ExportSecret>, EngineError> {
        let value: Option<SecretString> = self
            .vault_secret(SecretRef { connection, kind }, true)
            .await?;
        Ok(value.map(|value| ExportSecret::new(value.expose_secret())))
    }
}
