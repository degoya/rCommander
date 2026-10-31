//! `config.export` (RC-080-19): the chosen parts into one `.rcexport` file, unencrypted or
//! sealed with an export password of its own (`rc_vault::seal_export`).
//!
//! Secrets go only with the connections and only when asked: into a sealed file as a matter of
//! course, into an unencrypted one only after the person confirmed its warning
//! (`plain_secrets`). The vault is asked only then - an export without secrets never touches it.
//! Never exported: the MCP token and policy and the vault's key (no schema rows), the window's
//! geometry, an installation's own id, and hotlist items or tabs on a server or inside an
//! archive, which name this machine's connections. The file is written as a backup is - a fresh
//! temporary name beside it, owner-only, then renamed into place (`rc_db::write_private_file`);
//! the parts and the encryption chosen are kept for the next export.

use std::collections::HashMap;

use rc_db::{
    Db,
    connections::{ConnectionOrigin, FolderNode},
};
use rc_vault::{ExposeSecret as _, KdfParams, MasterPassword, SecretString};
use rc_vfs::{BackendId, VfsPath};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{
    ConfigExportRequest, Connection, ConnectionDraft, Engine, EngineError, LoginScriptInfo,
    Outcome, PanelId, SecretKind, SecretRef,
    exchange::{
        ConnectionsPart, Content, ExchangeError, ExportFile, ExportParts, ExportReport,
        ExportSecret, ExportedCertificate, ExportedConnection, ExportedHostKey,
        ExportedHotlistItem, ExportedScript, ExportedScriptSecret, ExportedSetting, ExportedSource,
        ExportedTab, FORMAT, FORMAT_VERSION, HotlistPart, SOURCE_PREFIX, SealedFile, TrustPart,
    },
    settings::{
        EXPORT_CONNECTIONS_KEY, EXPORT_ENCRYPTED_KEY, EXPORT_HOTLIST_KEY, EXPORT_SETTINGS_KEY,
        EXPORT_TRUST_KEY, SETTINGS, stored_setting,
    },
};

/// argon2id's cost for a sealed export: the vault's; cheap in the tests.
#[cfg(not(test))]
const EXPORT_KDF: KdfParams = KdfParams::DEFAULT;
#[cfg(test)]
const EXPORT_KDF: KdfParams = KdfParams {
    m_cost_kib: 64,
    t_cost: 1,
    p_cost: 1,
};

impl Engine {
    /// `config.export`: `GuiOnly`.
    pub(crate) async fn export_config(
        &self,
        request: &ConfigExportRequest,
    ) -> Result<Outcome, EngineError> {
        let parts = request.parts;
        if !parts.any() {
            return Err(ExchangeError::NothingChosen.into());
        }
        let password = match &request.password {
            Some(password) if password.is_empty() => {
                return Err(ExchangeError::PasswordEmpty.into());
            }
            Some(password) => Some(MasterPassword::from(password.secret().clone())),
            None => None,
        };
        let secrets = request.secrets && parts.connections;
        if secrets && password.is_none() && !request.plain_secrets {
            return Err(ExchangeError::SecretsUnencrypted.into());
        }
        let path = request.path.trim().to_owned();
        let unwritable = |source| ExchangeError::TargetUnwritable {
            path: path.clone(),
            source,
        };
        if path.is_empty() {
            return Err(unwritable(std::io::Error::other("no file named")).into());
        }
        if !request.overwrite && tokio::fs::try_exists(&path).await.unwrap_or(true) {
            return Err(ExchangeError::TargetExists { path: path.clone() }.into());
        }
        let db = self.db()?;
        let mut report = ExportReport {
            path: path.clone(),
            encrypted: password.is_some(),
            secrets,
            ..ExportReport::default()
        };
        let mut content = Content {
            origin: db.installation_id().await?,
            secrets,
            ..Content::default()
        };
        if parts.settings {
            let settings = export_settings(db).await;
            report.settings = settings.len();
            content.settings = Some(settings);
            content.themes = Some(super::exchange_themes::export_themes(db).await);
        }
        if parts.connections {
            let part = self
                .export_connections(db, &content.origin, secrets)
                .await?;
            report.connections = part.connections.len();
            report.login_scripts = part.login_scripts.len();
            content.connections = Some(part);
        }
        if parts.trust {
            let part = export_trust(db).await?;
            report.host_keys = part.host_keys.len();
            report.certificates = part.certificates.len();
            content.trust = Some(part);
        }
        if parts.hotlist {
            let (part, skipped) = self.export_hotlist(db).await?;
            report.hotlist = part.items.len();
            report.tabs = part.tabs.len();
            report.skipped = skipped;
            content.hotlist = Some(part);
        }
        let created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let encode = |error: serde_json::Error| unwritable(std::io::Error::other(error));
        let file = match &password {
            Some(password) => {
                let plain = Zeroizing::new(serde_json::to_vec(&content).map_err(encode)?);
                drop(content);
                let sealed = rc_vault::seal_export(password, EXPORT_KDF, &plain).await?;
                ExportFile {
                    format: FORMAT.to_owned(),
                    version: FORMAT_VERSION,
                    created_at,
                    content: None,
                    sealed: Some(SealedFile::of(&sealed)),
                }
            }
            None => ExportFile {
                format: FORMAT.to_owned(),
                version: FORMAT_VERSION,
                created_at,
                content: Some(content),
                sealed: None,
            },
        };
        let bytes = Zeroizing::new(serde_json::to_vec_pretty(&file).map_err(encode)?);
        drop(file);
        report.bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        write_file(&path, bytes, request.overwrite).await?;
        self.remember_export_choice(parts, report.encrypted).await;
        tracing::debug!(
            encrypted = report.encrypted,
            secrets = report.secrets,
            bytes = report.bytes,
            "settings and connections exported"
        );
        Ok(Outcome::ConfigExported(report))
    }

    /// The parts and the encryption of this export, for the next one; a failure only logged.
    async fn remember_export_choice(&self, parts: ExportParts, encrypted: bool) {
        for (key, on) in [
            (EXPORT_SETTINGS_KEY, parts.settings),
            (EXPORT_CONNECTIONS_KEY, parts.connections),
            (EXPORT_TRUST_KEY, parts.trust),
            (EXPORT_HOTLIST_KEY, parts.hotlist),
            (EXPORT_ENCRYPTED_KEY, encrypted),
        ] {
            if let Err(error) = self.set_protected_setting(key, &Value::Bool(on)).await {
                tracing::warn!(key, code = error.code(), "an export choice was not kept");
            }
        }
    }

    /// The connection tree, its login scripts and - with `secrets` - their secrets.
    async fn export_connections(
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
        let mut part = ConnectionsPart::default();
        for (folders, connection) in listed {
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

    /// The hotlist and the tabs on this machine's file systems, and how many were left out.
    async fn export_hotlist(&self, db: &Db) -> Result<(HotlistPart, usize), EngineError> {
        let items = db.hotlist().await?;
        let parents: HashMap<i64, (Option<i64>, String)> = items
            .iter()
            .map(|item| (item.id, (item.parent_id, item.name.clone())))
            .collect();
        let mut part = HotlistPart::default();
        let mut skipped = 0;
        for item in &items {
            let mut folders = Vec::new();
            let mut parent = item.parent_id;
            // The hotlist nests at most `HOTLIST_MAX_DEPTH`; the bound also stops a loop.
            while let Some(id) = parent.filter(|_| folders.len() <= rc_db::HOTLIST_MAX_DEPTH) {
                let Some((above, name)) = parents.get(&id) else {
                    break;
                };
                folders.insert(0, name.clone());
                parent = *above;
            }
            let location = item.location.as_deref().map(local_place);
            let target = item.target.as_deref().map(local_place);
            if matches!(location, Some(None)) || matches!(target, Some(None)) {
                skipped += 1;
                continue;
            }
            part.items.push(ExportedHotlistItem {
                folders,
                name: item.name.clone(),
                location: location.flatten(),
                target: target.flatten(),
            });
        }
        for panel in PanelId::BOTH {
            for tab in self.tabs(panel) {
                if tab.path.backend != BackendId::Local {
                    skipped += 1;
                    continue;
                }
                part.tabs.push(ExportedTab {
                    panel,
                    location: tab.path,
                    locked: tab.locked,
                    active: tab.active,
                });
            }
        }
        Ok((part, skipped))
    }
}

/// Every setting of the schema with the value in force.
async fn export_settings(db: &Db) -> Vec<ExportedSetting> {
    let mut settings = Vec::with_capacity(SETTINGS.len());
    for spec in SETTINGS {
        if let Some(value) = stored_setting(db, spec.key).await {
            settings.push(ExportedSetting {
                setting: spec.key.to_owned(),
                value,
            });
        }
    }
    settings
}

/// The trusted host keys and certificates.
async fn export_trust(db: &Db) -> Result<TrustPart, EngineError> {
    let host_keys = db
        .known_hosts()
        .await?
        .into_iter()
        .map(|key| ExportedHostKey {
            host: key.host,
            port: key.port,
            algorithm: key.algorithm,
            public_key: key.public_key,
        })
        .collect();
    let certificates = db
        .trusted_certificates()
        .await?
        .into_iter()
        .map(|cert| ExportedCertificate {
            host: cert.host,
            port: cert.port,
            fingerprint: cert.fingerprint_sha256,
            subject: cert.subject,
            issuer: cert.issuer,
            not_after: cert.not_after,
        })
        .collect();
    Ok(TrustPart {
        host_keys,
        certificates,
    })
}

/// Every connection below `nodes` with the folders it sits in.
fn flatten(nodes: &[FolderNode], above: &[String], out: &mut Vec<(Vec<String>, Connection)>) {
    for node in nodes {
        let mut path = above.to_vec();
        path.push(node.folder.name.clone());
        for connection in &node.connections {
            out.push((path.clone(), connection.clone()));
        }
        flatten(&node.folders, &path, out);
    }
}

/// A connection as the draft that makes it again.
pub(crate) fn draft_of(connection: &Connection) -> ConnectionDraft {
    ConnectionDraft {
        name: connection.name.clone(),
        kind: connection.kind,
        host: connection.host.clone(),
        port: Some(i64::from(connection.port)),
        username: connection.username.clone(),
        auth_method: connection.auth_method,
        key_path: connection.key_path.clone(),
        default_view: connection.default_view,
        start_directory: connection.start_directory.clone(),
        notes: connection.notes.clone(),
        ftp_data_mode: connection.ftp_data_mode,
        ftp_encoding: connection.ftp_encoding,
        theme_light: connection.theme_light.clone(),
        theme_dark: connection.theme_dark.clone(),
    }
}

/// A stored location when it is on this machine's file systems; `None` for one that does not
/// read or lies on a server or in an archive.
fn local_place(json: &str) -> Option<VfsPath> {
    serde_json::from_str::<VfsPath>(json)
        .ok()
        .filter(|place| place.backend == BackendId::Local)
}

/// `bytes` into the file `path` as a backup is written (`rc_db::write_private_file`): a fresh
/// temporary name that refuses an existing file or link, owner-only on Unix, on disk before it
/// takes the name; off the async threads.
async fn write_file(
    path: &str,
    bytes: Zeroizing<Vec<u8>>,
    overwrite: bool,
) -> Result<(), EngineError> {
    let target = std::path::PathBuf::from(path);
    let written =
        tokio::task::spawn_blocking(move || rc_db::write_private_file(&target, &bytes, overwrite))
            .await
            .map_err(|error| std::io::Error::other(error.to_string()));
    let failed = |source| ExchangeError::TargetUnwritable {
        path: path.to_owned(),
        source,
    };
    match written {
        Ok(Ok(())) => Ok(()),
        Ok(Err(rc_db::BackupError::TargetExists { .. })) => Err(ExchangeError::TargetExists {
            path: path.to_owned(),
        }
        .into()),
        Ok(Err(rc_db::BackupError::Target { source, .. })) => Err(failed(source).into()),
        Ok(Err(other)) => Err(failed(std::io::Error::other(other.to_string())).into()),
        Err(source) => Err(failed(source).into()),
    }
}

#[cfg(test)]
#[path = "exchange_export_tests.rs"]
mod tests;
